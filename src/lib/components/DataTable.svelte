<script>
  import { tick, onDestroy, untrack } from "svelte";
  import { fade } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { zoomState } from '$lib/stores/canvas-zoom.svelte.js'
  // Zoom is driven through the app-level settings so the canvas scales together
  // with the rest of the UI (applySettings mirrors the app zoom into zoomState).
  import { increaseZoom, decreaseZoom, resetZoom, appPreviewDml, appTableStyle, TABLE_STYLES, normalizeTableStyle, appVimMode, appTableAlign, appNativeScroll, appRowSpacing, appZebraRows, rowSpacingHeight, appNumberGrouping, appHighlightActiveRow, appGridFontSize, appImagePreview, appOpenUrlsOnClick, appRowNumbers } from '$lib/stores/settings.js'
  import { createSmoothScroll, wheelPixels } from '$lib/smooth-scroll.js'
  import { isJsonColumnType } from '$lib/cell-expand.js'
  import { readClipboardText } from '$lib/clipboard.js'
  import { setVimSubMode } from '$lib/vim/vim.js'
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import ArrowUpDown from "@lucide/svelte/icons/arrow-up-down";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import ListFilter from "@lucide/svelte/icons/list-filter";
import FilterX from "@lucide/svelte/icons/filter-x";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronsLeft from "@lucide/svelte/icons/chevrons-left";
  import ChevronsRight from "@lucide/svelte/icons/chevrons-right";
  import Palette from "@lucide/svelte/icons/palette";
  import Tag from "@lucide/svelte/icons/tag";
  import Ban from "@lucide/svelte/icons/ban";
  import Copy from "@lucide/svelte/icons/copy";
  import MoveHorizontal from "@lucide/svelte/icons/move-horizontal";
  import Type from "@lucide/svelte/icons/type";
  import CopyPlus from "@lucide/svelte/icons/copy-plus";
  import ArrowUpFromLine from "@lucide/svelte/icons/arrow-up-from-line";
  import ArrowDownFromLine from "@lucide/svelte/icons/arrow-down-from-line";
  import Pencil from "@lucide/svelte/icons/pencil";
  import CircleSlash from "@lucide/svelte/icons/circle-slash";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Braces from "@lucide/svelte/icons/braces";
  import CheckSquare from "@lucide/svelte/icons/check-square";
  import PanelRight from "@lucide/svelte/icons/panel-right";
  import PanelBottom from "@lucide/svelte/icons/panel-bottom";
  import Pin from "@lucide/svelte/icons/pin";
  import PinOff from "@lucide/svelte/icons/pin-off";
  import Table2 from "@lucide/svelte/icons/table-2";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import {
    findForeignKeyForColumn,
    foreignKeyTargetLabel,
  } from "$lib/foreign-key-nav.js";
  import TableLoading from "./TableLoading.svelte";
  import {
    loadColumnWidths,
    saveColumnWidths,
  } from "$lib/stores/table-column-widths.js";
  import {
    clampColumnWidth,
    columnAlignsRight,
    defaultColumnWidth,
  } from "$lib/table-column-widths.js";
  import { formatCompactCount } from "$lib/table-list.js";
  import { keycaps } from "$lib/shortcuts.js";
  import { cn } from "$lib/utils.js";
  import { buildQuickFilter } from "$lib/quick-filter.js";
  import {
    formatJsonValue,
    formatNormalValue,
    rowToRecord,
  } from "$lib/row-inspector.js";
  import {
    getColumnEnumValues,
    isBooleanType,
    isEditableType,
    parseCellInput,
    valueToEditString,
    isAutoColumn,
    insertOmitBehaviour,
    buildInsertPayload,
    isDateTimeType,
    isDateOnlyType,
    isTimeOnlyType,
    oversizeCellInfo,
    oversizeCellText,
    formatByteSize,
  } from "$lib/cell-value.js";
  import {
    defaultInsertDraft,
    shouldUseDateTimePicker,
    generateUuid,
    generateCuid,
    nowDateTimeLocal,
    nowDateOnly,
    nowTimeOnly,
  } from "$lib/insert-field.js";
  import { cellLinkHref, cellUrlType } from "$lib/cell-display.js";
  import InsertValuePicker from "./InsertValuePicker.svelte";
  import {
    buildUpdateStatements,
    buildDeleteStatements,
    buildInsertStatements,
  } from "$lib/dml-preview.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import DmlReviewPanel from "./DmlReviewPanel.svelte";
  import {
    savePendingChanges,
    loadPendingChanges,
    clearPendingChanges,
  } from "$lib/stores/pending-table-edits.js";
  import { formatCellValue, transformsFor, transformById, enabledGeneratorGroups, linkifyValue, statsNeeded, annotatorEnabled, anyDisplayExtEnabled } from "$lib/plugins/registry.js";
  import { pluginState, isPluginEnabled } from "$lib/stores/plugins.js";
  import { externalFormatVersion } from "$lib/plugins/external/host.js";
  import { isImageUrl } from "$lib/plugins/extensions/cell-transforms.js";
  import { t } from "$lib/i18n.js";
  import Wand2 from "@lucide/svelte/icons/wand-2";
  import Dices from "@lucide/svelte/icons/dices";
  import ClipboardPaste from "@lucide/svelte/icons/clipboard-paste";
  import Eraser from "@lucide/svelte/icons/eraser";
  import Columns3 from "@lucide/svelte/icons/columns-3";
  import CalendarDays from "@lucide/svelte/icons/calendar-days";
  import Hash from "@lucide/svelte/icons/hash";
  import Clock from "@lucide/svelte/icons/clock";
  import MediaLightbox from "./MediaLightbox.svelte";
  import CellEditorPanel from "./CellEditorPanel.svelte";
  import RowExpandViewer from "./RowExpandViewer.svelte";
  import ArrayCellEditor from "./ArrayCellEditor.svelte";
  import VectorCellViewer from "./VectorCellViewer.svelte";
  import { vectorSummary } from "$lib/vector-cell.js";
  import GeometryCellViewer from "./GeometryCellViewer.svelte";
  import { isGeometryType, geometrySummary } from "$lib/geometry-cell.js";
  import FkSubviewPanel from "./FkSubviewPanel.svelte";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Check from "@lucide/svelte/icons/check";
  import Loader from "@lucide/svelte/icons/loader";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import X from "@lucide/svelte/icons/x";
  import DateTimePicker from "./DateTimePicker.svelte";
  import SearchableMenu from "./SearchableMenu.svelte";
  import Icon from "./Icon.svelte";
  import ColumnStatsPanel from "./ColumnStatsPanel.svelte";
  import BarChart2 from "@lucide/svelte/icons/bar-chart-2";
  import VirtualColumnsPanel from "./VirtualColumnsPanel.svelte";
  import { virtualColumnsStore } from "$lib/stores/virtual-columns.js";
  import { bindExpr, looksLikeUrl } from "$lib/virtual-column.js";
  import {
    createColorReader,
    withAlpha,
    drawIcon,
    roundRect,
    roundRectPath,
    drawCheckbox,
    computeColumnGeometry,
    colDrawnX,
    colAtX,
    computeRowTops,
    rowTopOf,
    totalRowsHeight,
    rowAtContentY,
    rowIndexAtY,
  } from "$lib/canvas-table.js";

  // Local derived so all $derived layout constants track it reactively.
  // Embedded results sit inside a chat bubble or a console pane, where the
  // full-size grid reads as oversized next to the surrounding text. Scaling the
  // canvas zoom scales every layout constant AND the fonts together, so the
  // smaller grid stays internally proportioned instead of just having tighter rows.
  const EMBEDDED_SCALE = 0.85
  const canvasZoom = $derived(zoomState.value * (embedded ? EMBEDDED_SCALE : 1))

  let {
    columns = [],
    rows = [],
    loading = false,
    primaryKey = [],
    foreignKeys = [],
    saving = false,
    selected = $bindable(new Set()),
    /** @type {number | null} */
    focusedRow = $bindable(null),
    /** Visible-column index of the focused cell (null = no cell focus). Bindable
     *  so the parent can persist the whole cursor per tab and put it back on
     *  back/forward - a row without its column only restores half the position.
     *  @type {number | null} */
    focusedCol = $bindable(null),
    /** @type {number | null} */
    inspectorRow = $bindable(null),
    /** @type {{ rowIdx: number, colIdx: number, draft: string, original: string } | null} */
    editingCell = $bindable(null),
    /**
     * @param {{ rowIdx: number, colIdx: number, value: unknown }} detail
     * @returns {Promise<void>}
     */
    onsave = async () => {},
    /**
     * @param {{ rowIndices: number[] }} detail
     * @returns {Promise<void>}
     */
    ondelete = async () => {},
    /** @param {{ rowIdx: number, colIdx: number }} detail */
    onfollowforeignkey = () => {},
    /** Compact layout for AI chat / nested panels */
    embedded = false,
    showSelection = true,
    showRowExpand = true,
    /** Persist column widths per table, e.g. "public.users" */
    columnWidthsKey = undefined,
    /** Active connection id - scopes ALL per-table persistence (widths, staged
     *  edits, highlights, transforms, virtual columns) so state from one database
     *  never leaks into another table of the same schema.table name on a different
     *  connection. */
    connectionId = '',
    /** Vim mode `/` - asks the parent to focus the row-search input (the toolbar
     *  owns it, not the grid). */
    onrequestsearch = () => {},
    /** Schema + table name used for INSERT statement generation */
    schema = '',
    tableName = '',
    /** Engine family - drives identifier quoting in the DML preview. */
    dialect = /** @type {import('$lib/dml-preview.js').Dialect} */ ('postgres'),
    /**
     * The database's completion hints (schemas, tables, columns), built on
     * demand - the review dock asks only while it is open.
     * @type {(() => import('$lib/sql-complete-data.js').SqlSchemaHints) | null}
     */
    getsqlhints = null,
    /** Set of column names to hide. Controlled externally (toolbar). */
    hiddenColumns = /** @type {Set<string>} */ (new Set()),
    /**
     * Indexes for this table (from listIndexes, already filtered to the current table).
     * Used to show index/unique badges on column headers.
     * @type {{ name: string, tableName: string, columns: string, indexType: string, isUnique: boolean, isPrimary: boolean }[]}
     */
    indexes = [],
    /** Column names pinned to the left. Bindable so the parent can persist. */
    pinnedColumns = $bindable(/** @type {Set<string>} */ (new Set())),
    /** Active sort. null = unsorted. */
    rowSort = /** @type {{ column: string, direction: 'asc' | 'desc' } | null} */ (null),
    /** Secondary sort keys (multi-column sort); primary is rowSort. @type {{ column: string, direction: 'asc' | 'desc' }[]} */
    rowSortMore = [],
    /** Called on header sort. Emits the full ordered key list ([] clears). */
    onsortchange = /** @type {(sorts: { column: string, direction: 'asc' | 'desc' }[]) => void} */ (() => {}),
    /** Number of staged (unsaved) cell edits. Bindable so the StatusBar can show Apply/Reset. */
    pendingEditCount = $bindable(0),
    /** Assigned by this component; the parent calls these to flush / discard staged edits. */
    applyEdits = $bindable(/** @type {() => void | Promise<void>} */ (() => {})),
    /** Copy the staged changes as SQL instead of running them. */
    copyEditsSql = $bindable(/** @type {() => void | Promise<void>} */ (() => {})),
    resetEdits = $bindable(/** @type {() => void} */ (() => {})),
    /** Assigned by this component; the parent (StatusBar) calls these to jump the
     *  table to the top / bottom. */
    scrollToTop = $bindable(/** @type {() => void} */ (() => {})),
    scrollToBottom = $bindable(/** @type {() => void} */ (() => {})),
    /** Assigned by this component; the parent (StatusBar) calls these to jump the
     *  table to the far left / right when it scrolls horizontally. */
    scrollToLeft = $bindable(/** @type {() => void} */ (() => {})),
    scrollToRight = $bindable(/** @type {() => void} */ (() => {})),
    /** Bindable: true when the grid content is wider than the viewport (so the
     *  parent can show the horizontal go-to-edge controls). */
    canScrollHorizontally = $bindable(false),
    /** Assigned by this component; the parent (toolbar "Jump to column" menu)
     *  calls focusColumn(name) to scroll a column into view and briefly
     *  highlight it. */
    focusColumn = $bindable(/** @type {(name: string) => void} */ (() => {})),
    /** Assigned by this component; the parent calls focusCell(row, col) to put the
     *  cursor on a specific cell and bring it on screen - used when restoring a
     *  back/forward position. Indices are clamped to the loaded data. */
    focusCell = $bindable(/** @type {(row: number, col?: number | null) => void} */ (() => {})),
    /** Assigned by this component so the parent can persist/restore scroll per
     *  tab. getScroll() reads the live position; applyScroll() restores it once
     *  layout settles. */
    getScroll = $bindable(/** @type {() => { left: number, top: number }} */ (() => ({ left: 0, top: 0 }))),
    applyScroll = $bindable(/** @type {(pos: { left?: number, top?: number }) => void} */ (() => {})),
    /** Background/snapshot panes seed their scroll offset and open row-expand
     *  panels from the tab's saved state, so a defocused split pane keeps its
     *  position and expanded rows instead of resetting to the top/collapsed.
     *  Null for the live grid, which persists via getScroll/getExpanded. */
    initialScroll = /** @type {{ left?: number, top?: number } | null} */ (null),
    initialExpandedRows = /** @type {number[] | null} */ (null),
    /** The tab was opened by following a foreign key: the moment exactly one row
     *  lands, it opens as JSON. One row alone says little, its panel says it all.
     *  Off via Settings → Data grid → Expand single related row. */
    expandSingleRow = false,
    /** Assigned by this component so the parent can persist the open row-expand
     *  panels per tab (mirrors getScroll). Returns the open row indices. */
    getExpanded = $bindable(/** @type {() => number[]} */ (() => [])),
    /** Called when user picks "Filter by this column" from the column header context menu. */
    onfiltercolumn = /** @type {(colName: string) => void} */ (() => {}),
    /** Called when user right-clicks a cell and picks "Filter by value" or "Exclude value". */
    onfilterbyvalue = /** @type {(colName: string, value: unknown, exclude?: boolean) => void} */ (() => {}),
    onquickfilter = /** @type {(colName: string, op: string, value: string) => void} */ (() => {}),
    /** Called when user picks "Hide column" from the column header context menu. */
    onhidecolumn = /** @type {(colName: string) => void} */ (() => {}),
    /**
     * Reverse FK relationships (tables that reference this table).
     * Rendered as virtual badge columns at the right.
     * @type {Array<{ fromSchema:string, fromTable:string, fromColumns:string[], toColumns:string[], constraintName:string }>}
     */
    incomingForeignKeys = [],
    /** Fetch related rows for an inline FK sub-view. Returns { columns, rows, error? }. */
    onfetchrelatedrows = /** @type {(detail: any) => Promise<{ columns: any[], rows: any[], error?: string }>} */ (async () => ({ columns: [], rows: [] })),
    /**
     * Load one capped cell in full. Wide columns arrive as a preview so a page
     * of half-megabyte values stays openable; this is how the dock gets the
     * real thing when someone asks for it. Null when the engine cannot.
     * @type {null | ((detail: { rowIdx: number, colIdx: number }) => Promise<{ text: string, bytes: number, truncated: boolean }>)}
     */
    onfetchcellvalue = null,
    /**
     * Fetch one capped cell AND write it into the row, for the in-cell Load
     * button. Separate from `onfetchcellvalue`, which hands the text back for
     * the dock to show without touching the page.
     * @type {null | ((detail: { rowIdx: number, colIdx: number }) => Promise<void>)}
     */
    onloadcellvalue = null,
    /** Called when the user confirms the new row draft. Receives the validated values. */
    oninsertrow = /** @type {(values: Record<string, unknown>) => Promise<void>} */ (async () => {}),
    /**
     * Execute raw SQL the user hand-edited in the DML preview, then refetch. Only
     * invoked when the previewed SQL was actually changed - the unedited path still
     * runs through the structured per-cell writes (`onsave`/`ondelete`/`oninsertrow`).
     * @type {(sql: string) => Promise<void>}
     */
    onexecutesql = /** @type {(sql: string) => Promise<void>} */ (async () => {}),
    /** True while the insert is in flight - disables the draft row inputs. */
    insertSaving = false,
    /** Assigned by this component so the parent can trigger beginInsertRow(). */
    beginInsertRow = $bindable(/** @type {() => void} */ (() => {})),
    /** Assigned by this component so the parent (⌘⌫ / toolbar) can stage the
     *  selected rows for deletion instead of deleting them immediately. */
    stageDeleteSelected = $bindable(/** @type {() => void} */ (() => {})),
    /** When true all write operations (edit, delete, insert) are disabled. */
    readonly = false,
    /** Bindable: controls whether the virtual columns management panel is open. */
    vcolPanelOpen = $bindable(false),
    /** Incremented by the parent when a fresh page of rows is applied
     *  (page/filter/sort/search change). On change the table jumps its scroll
     *  and virtual window back to the top. */
    reloadToken = 0,
    /** Bumped by the parent when window data is spliced into the (sparse) rows
     *  array without changing its identity - triggers a redraw WITHOUT resetting
     *  scroll (unlike reloadToken). */
    dataVersion = 0,
    /** Windowed mode: `rows` is sparse (length = total, only near-viewport rows
     *  loaded). Undefined rows render as a loading skeleton and the table reports
     *  its visible range via onvisiblerange so the parent can fetch/evict. */
    windowed = false,
    /**
     * Absolute index of `rows[0]` in the result set, so the row-number gutter
     * counts from the page rather than from the screen: row 201 reads 201 on
     * page 3, not 1. Windowed mode addresses rows absolutely already, so it
     * passes 0.
     */
    rowNumberOffset = 0,
    /** Called (on change) with the currently visible row range in windowed mode. */
    onvisiblerange = /** @type {(start: number, end: number) => void} */ (() => {}),
    /** Windowed mode: what the parent's window fetcher is doing, so the grid can
     *  say what it is waiting for instead of shimmering silently.
     *  @type {{ slow: boolean, failed: boolean } | null} */
    windowStatus = null,
    /** Retry the windows that gave up (the loading pill's Retry button). */
    onretrywindows = /** @type {() => void} */ (() => {}),
    /** Infinite scroll mode - when true the table fires onloadmore near the bottom. */
    infiniteScroll = false,
    /** True while an incremental "load more" fetch is in flight. */
    loadingMore = false,
    /** Called when the user scrolls near the bottom in infinite scroll mode. */
    onloadmore = /** @type {() => void} */ (() => {}),
    /** Called when the cell cursor moves because the user *aimed* at a cell (a
     *  click) rather than roaming with the arrow keys. Lets the parent record a
     *  back/forward position for a short, deliberate move it would otherwise
     *  treat as roaming. */
    /** True when every row has been loaded in infinite scroll mode (no more pages). */
    endOfResults = false,
    /** Active row-search query (toolbar search). Matched substrings are
     *  highlighted in the drawn cell text. */
    searchQuery = '',
    /**
     * How that query matched, so the highlight agrees with the rows.
     * @type {{ matchCase?: boolean, wholeWord?: boolean, regex?: boolean }}
     */
    searchOptions = {},
  } = $props();

  /**
   * Staged cell edits not yet written to the database, keyed by "rowIdx:colIdx".
   * The cell shows the staged value (marked dirty) until the user clicks Apply.
   * @type {Map<string, { rowIdx: number, colIdx: number, value: unknown, original: unknown }>}
   */
  // Restore any changes staged for this table before the component last unmounted
  // (switching to a SQL/AI tab, or another table tab, tears DataTable down).
  const _restoredPending = untrack(() => loadPendingChanges(columnWidthsKey ?? ''));

  // Stable table key for persistence. `columnWidthsKey` derives from the parent's
  // `activeTable`, which is nulled during teardown when switching to a SQL/AI tab -
  // so reading it in onDestroy would lose the key. Track the last non-empty value.
  let _persistKey = untrack(() => columnWidthsKey ?? '');
  $effect(() => { if (columnWidthsKey) _persistKey = columnWidthsKey; });

  let pendingEdits = $state(_restoredPending.edits);
  /** Cheap gate so per-cell staged-edit lookups are skipped entirely when there
   *  are no unsaved edits (the common case) - avoids a string alloc + Map.get
   *  on every cell of large tables. */
  const hasPendingEdits = $derived(pendingEdits.size > 0);
  /** Row indices with a staged edit - lets the draw loop skip the per-cell key
   *  string for every row that isn't edited (usually all of them). */
  const _editedRowSet = $derived.by(() => {
    /** @type {Set<number>} */
    const s = new Set();
    for (const e of pendingEdits.values()) s.add(e.rowIdx);
    return s;
  });

  /**
   * Row indices staged for deletion - shown with a red diff marker until Apply.
   * Kept separate from `pendingEdits` so a row can be edited then deleted, and
   * so the gutter/row rendering can distinguish the two.
   * @type {Set<number>}
   */
  let pendingDeletes = $state(_restoredPending.deletes);
  const hasPendingDeletes = $derived(pendingDeletes.size > 0);
  /** Any unsaved change (edit or delete) - drives the tab/close guards. */
  const hasPendingChanges = $derived(pendingEdits.size > 0 || pendingDeletes.size > 0);

  /**
   * DML review dock. Non-null while open. Every write path (apply staged
   * edits, insert a new row, delete rows) routes through this so the user can
   * review the exact SQL before it runs. `run` performs the actual write.
   * `rebuild`, when set, regenerates the review from the current staged state:
   * the grid stays editable under the dock, and `run` writes whatever is staged
   * at the time, so the SQL on screen has to follow it.
   * @typedef {{ kind: 'update' | 'insert' | 'delete', title: string, description: string, statements: string[], confirmLabel: string, destructive: boolean, run: () => Promise<void>, rebuild?: () => DmlReview | null }} DmlReview
   * @type {DmlReview | null}
   */
  let dmlPreview = $state(null);
  /** @type {{ focus: () => void } | null} */
  let dmlPanel = $state(null);
  /** True while the confirmed write is in flight. */
  let dmlPreviewRunning = $state(false);
  /** Prettified SQL shown in the (editable) preview editor. Bound to the editor. */
  let dmlEditedSql = $state("");
  /** The pristine prettified SQL, to detect whether the user edited it. */
  let dmlOriginalSql = $state("");
  /** True once the user has changed the previewed SQL - switches Apply to raw exec. */
  const dmlWasEdited = $derived(dmlEditedSql.trim() !== dmlOriginalSql.trim());

  /** @type {import('$lib/dml-preview.js').DmlContext} */
  const dmlContext = $derived({ dialect, schema, table: tableName, columns, primaryKey });

  /**
   * Completion for the review: the whole database's hints, with this table's
   * columns carrying their types. Built only while the review is open.
   */
  const dmlReviewOpen = $derived(dmlPreview !== null);
  const dmlCompletion = $derived.by(() => {
    if (!dmlReviewOpen) return {};
    const base = getsqlhints?.() ?? {};
    const own = columns.map((c) => ({ name: c.name, type: c.dataType ?? c.data_type ?? "" }));
    const tables = base.tables ?? [];
    return {
      ...base,
      tables: tables.includes(tableName) ? tables : [...tables, tableName],
      columnsByTable: {
        ...base.columnsByTable,
        [tableName]: own,
        ...(schema ? { [`${schema}.${tableName}`]: own } : {}),
      },
    };
  });

  /**
   * Route a write through the confirm dialog, or run it straight away when the
   * "Preview SQL before applying" setting is off.
   * @param {DmlReview} config
   */
  function requestWrite(config) {
    if (!$appPreviewDml) { void config.run(); return; }
    // Apply again while the review is up: never confirm (a double-click would
    // skip the review) and never throw away SQL the user has typed.
    if (dmlPreview && (dmlPreviewRunning || dmlWasEdited)) { dmlPanel?.focus(); return; }
    showDmlReview(config);
    dmlPanel?.focus();
  }

  // sql-formatter is ~250 KB, and this grid is on the startup path: it loads
  // once the app is idle instead, so a review still opens formatted, in the
  // same frame, and stays synchronous for the rebuild effect below.
  /** @type {((sql: string) => string) | null} */
  let formatSqlFn = null;
  /** @type {Promise<(sql: string) => string> | null} */
  let formatterLoad = null;
  function loadFormatter() {
    formatterLoad ??= import("$lib/format-sql.js").then((m) => (formatSqlFn = m.formatSql));
    return formatterLoad;
  }
  $effect(() => {
    const idle = window.requestIdleCallback ?? ((/** @type {() => void} */ cb) => setTimeout(cb, 1500));
    idle(() => void loadFormatter().catch(() => {}));
  });

  /** @param {DmlReview} config */
  function showDmlReview(config) {
    const wasEdited = dmlPreview !== null && dmlWasEdited;
    dmlPreview = config;
    // Prettify the generated statements for a readable, editable preview.
    const raw = config.statements.join("\n");
    dmlOriginalSql = formatSqlFn ? formatSqlFn(raw) : raw;
    if (!wasEdited) dmlEditedSql = dmlOriginalSql;
    if (formatSqlFn) return;
    // A review in the first moments after startup: the raw SQL until the
    // formatter lands, then the formatted one unless it was edited meanwhile.
    void loadFormatter().then((fmt) => {
      if (dmlPreview !== config || dmlOriginalSql !== raw) return;
      const pretty = fmt(raw);
      if (dmlEditedSql === raw) dmlEditedSql = pretty;
      dmlOriginalSql = pretty;
    }).catch(() => {});
  }

  /** @param {{ refocus?: boolean }} [opts] */
  function closeDmlPreview({ refocus = true } = {}) {
    if (dmlPreviewRunning) return;
    dmlPreview = null;
    if (refocus) tick().then(() => tableContainer?.focus({ preventScroll: true }));
  }

  // Keep the review in step with the grid under it (see `rebuild`).
  $effect(() => {
    void pendingEdits;
    void pendingDeletes;
    void newRowDrafts;
    untrack(() => {
      if (!dmlPreview?.rebuild || dmlPreviewRunning) return;
      const next = dmlPreview.rebuild();
      if (next) showDmlReview(next);
      else closeDmlPreview({ refocus: false });
    });
  });

  /** Run the previewed write, then close the review. */
  async function confirmDmlPreview() {
    if (!dmlPreview || dmlPreviewRunning) return;
    dmlPreviewRunning = true;
    try {
      if (dmlWasEdited) {
        // The user hand-edited the SQL - run it verbatim (as raw statements) and
        // refetch. Arbitrary edits can't be mapped back to the staged per-cell
        // model, so drop all staged edits/deletes and the inline insert draft.
        await onexecutesql(dmlEditedSql.trim());
        pendingEdits = new Map();
        pendingDeletes = new Set();
        pastEdits = [];
        futureEdits = [];
        savePendingChanges(_persistKey, pendingEdits, pendingDeletes);
        cancelNewRow();
      } else {
        await dmlPreview.run();
      }
      dmlPreviewRunning = false;
      closeDmlPreview();
    } finally {
      dmlPreviewRunning = false;
    }
  }

  /** @type {HTMLInputElement | HTMLSelectElement | HTMLButtonElement | null} */
  let editInput = $state(null);

  /**
   * Currently open FK sub-view (forward or reverse), or null.
   * @type {{ rowIdx:number, kind:'forward'|'reverse', label:string, data:{ loading:boolean, columns:any[], rows:any[], error:string|null } } | null}
   */
  let fkSubview = $state(null)

  // ── The dock follows the cell cursor ────────────────────────────────────────
  // Arrowing up and down a FK column left the dock showing the row it was opened
  // on, so the preview and the cursor disagreed about which row was being read -
  // and the only way to move it was to click another FK cell. It now re-queries
  // the same relation for whatever row the cursor lands on. Debounced: holding
  // the arrow key down should cost one query at the end of the run, not one per
  // row crossed.
  const FK_FOLLOW_DELAY = 140
  /** @type {ReturnType<typeof setTimeout> | null} */
  let _fkFollowTimer = null
  let _fkFollowSeq = 0

  // Across a row it follows the column too. A table whose columns are all
  // foreign keys is exactly where the dock earns its keep, and arrowing along
  // one used to keep showing the relation the dock was opened on: the cursor
  // said credits_credithistory and the dock said authtoken_token. Only the row
  // was watched, so a sideways move changed nothing.
  $effect(() => {
    const targetRow = focusedRow
    const targetVis = focusedCol
    const sv = fkSubview
    if (!sv || targetRow === null || rows[targetRow] === undefined) return
    const targetCol = targetVis === null ? -1 : visToActualColIdx(targetVis)
    // A reverse relation hangs off the row, not off any one column, so it keeps
    // following rows only. A forward one belongs to its column, and moving onto
    // a different foreign key is a request to see that one.
    const movedToOtherFk =
      sv.kind === 'forward' && targetCol >= 0 && targetCol !== sv.colIdx && !!_colCache[targetCol]?.fk
    if (targetRow === sv.rowIdx && !movedToOtherFk) return
    untrack(() => {
      if (_fkFollowTimer) clearTimeout(_fkFollowTimer)
      _fkFollowTimer = setTimeout(() => {
        _fkFollowTimer = null
        void followFkSubview(targetRow, movedToOtherFk ? targetCol : -1)
      }, FK_FOLLOW_DELAY)
    })
  })

  /**
   * Re-point the open related-rows dock at `idx`, keeping its relation and its
   * height. A NULL foreign key resolves to the panel's empty state rather than a
   * query that can only come back empty.
   * @param {number} idx
   * @param {number} [nextCol] Switch to this column's relation as well as this
   *   row; -1 or omitted keeps the relation the dock already has.
   */
  async function followFkSubview(idx, nextCol = -1) {
    const sv = fkSubview
    if (!sv || rows[idx] === undefined) return
    if (sv.rowIdx === idx && nextCol < 0) return
    const row = rows[idx] ?? []
    // Switching column switches the relation, and with it the label the settle
    // below checks itself against.
    const colIdx = nextCol >= 0 ? nextCol : (sv.colIdx ?? -1)
    const label =
      nextCol >= 0 && _colCache[nextCol]?.fk
        ? foreignKeyTargetLabel(_colCache[nextCol].fk)
        : sv.label
    const seq = ++_fkFollowSeq
    /** @param {{ columns?: any[], rows?: any[], error?: string | null }} res */
    const settle = (res) => {
      // A newer move (or a close, or a different relation) owns the dock now.
      if (seq !== _fkFollowSeq || fkSubview?.rowIdx !== idx || fkSubview?.label !== label) return
      fkSubview = { ...fkSubview, data: { loading: false, columns: res.columns ?? [], rows: res.rows ?? [], error: res.error ?? null } }
    }

    if (sv.kind === 'reverse') {
      const rel = sv.relInfo
      if (!rel) return
      fkSubview = { ...sv, rowIdx: idx, data: { loading: true, columns: [], rows: [], error: null } }
      settle(await onfetchrelatedrows({
        kind: 'reverse', fromSchema: rel.fromSchema, fromTable: rel.fromTable,
        fromColumns: rel.fromColumns, toColumns: rel.toColumns, row,
      }))
      return
    }

    const fk = _colCache[colIdx]?.fk ?? null
    if (!fk) return
    const value = row[colIdx]
    if (value === null || value === undefined) {
      fkSubview = { ...sv, rowIdx: idx, colIdx, label, data: { loading: false, columns: [], rows: [], error: null } }
      return
    }
    fkSubview = { ...sv, rowIdx: idx, colIdx, label, data: { loading: true, columns: [], rows: [], error: null } }
    settle(await onfetchrelatedrows({ kind: 'forward', fk, row }))
  }

  /**
   * Open a relationship as a view of its own - the related table, filtered to
   * this row. The dock's "Open in sub view" button lands here too, so the
   * gesture and the button cannot drift apart.
   * @param {number} rowIdx @param {any} vc a `virtualRelCols` entry
   * @param {{ newTab?: boolean }} [opts]
   */
  function openReverseFkFullView(rowIdx, vc, opts = {}) {
    if (rows[rowIdx] === undefined) return
    onfollowforeignkey({ rowIdx, colIdx: 0, reverseRel: vc, row: rows[rowIdx], newTab: opts.newTab === true })
  }

  /**
   * The relationship cell under a canvas point, or null. `rowIdx` is -1 when the
   * point is inside the column but not on a row (the header band, the slack under
   * the last row) - still a hit, because the click belongs to that column either
   * way and must not fall through to the grid behind it.
   * @param {number} x @param {number} y
   */
  function vrelHitAt(x, y) {
    if (y < HEADER_H || virtualRelCols.length === 0) return null
    const cx = x + _scrollLeft
    const vi = _vrelLayout.findIndex((vp) => cx >= vp.x && cx < vp.x + vp.w)
    if (vi < 0) return null
    const bodyY = y + _scrollTop - HEADER_H - insertRowOffset
    const r = rowAtContentY(rowTops, rows.length, ROW_HEIGHT, bodyY)
    return { vi, vc: virtualRelCols[vi], rowIdx: r?.inRowBody ? r.idx : -1 }
  }

  /**
   * Open (or close) the related-rows dock for a relationship cell. Shared by the
   * click path and the Enter key, so a relation opens the same way whichever
   * one you reach it with.
   * @param {number} rowIdx @param {any} vc a `virtualRelCols` entry
   */
  function toggleReverseFkSubview(rowIdx, vc) {
    if (fkSubview?.rowIdx === rowIdx && fkSubview?.kind === 'reverse' && fkSubview?.label === vc.label) {
      fkSubview = null
      return
    }
    // Opening FK sub-view: close JSON expand for the same row (mutually exclusive)
    if (expandedRows.has(rowIdx)) { const s = new Set(expandedRows); s.delete(rowIdx); expandedRows = s }
    // The cursor moves to the row being inspected - a real column's click does
    // this on the way past, and the dock follows the cursor, so a relation cell
    // that left it behind would drag the dock back.
    focusedRow = rowIdx
    // Park the cursor ON the relation cell, not on whatever column it was last
    // in: it is a navigable column now, and the cell you acted on is the cell
    // the cursor should be standing in.
    const relNavIdx = virtualRelCols.indexOf(vc)
    focusedCol = relNavIdx >= 0 ? visibleColumns.length + relNavIdx : (focusedCol ?? 0)
    const row = rows[rowIdx] ?? []
    fkSubview = { rowIdx, kind: 'reverse', label: vc.label, relInfo: vc, data: { loading: true, columns: [], rows: [], error: null } }
    scrollRowIntoViewBesideDock(rowIdx)
    void onfetchrelatedrows({ kind: 'reverse', fromSchema: vc.fromSchema, fromTable: vc.fromTable, fromColumns: vc.fromColumns, toColumns: vc.toColumns, row }).then(res => {
      if (fkSubview?.rowIdx !== rowIdx || fkSubview?.label !== vc.label) return
      fkSubview = { ...fkSubview, data: { loading: false, columns: res.columns ?? [], rows: res.rows ?? [], error: res.error ?? null } }
    })
  }

  onDestroy(() => {
    if (_fkFollowTimer) clearTimeout(_fkFollowTimer)
    if (_spinRaf) cancelAnimationFrame(_spinRaf)
  })

  // ── Related-rows dock (bottom panel) ────────────────────────────────────────
  // The FK sub-view renders docked below the scroll container - a fixed-height
  // drawer with its own internal scroll - instead of inline between rows (which
  // made grid scrolling fight the panel). Height is user-resizable + persisted.
  const FK_DOCK_MIN = 120, FK_DOCK_MAX = 600
  /** @param {string} key @param {number} fallback */
  function loadDockHeight(key, fallback) {
    try {
      const n = Number(localStorage.getItem(key))
      if (Number.isFinite(n) && n >= FK_DOCK_MIN && n <= FK_DOCK_MAX) return n
    } catch {}
    return fallback
  }
  let fkDockHeight = $state(loadDockHeight('stroke:fk-dock-height', 260))
  /** The full-size cell editor is the second dock, and remembers its own height. */
  let cellDockHeight = $state(loadDockHeight('stroke:cell-dock-height', 220))
  /** The DML review is the third, with its own height too. */
  let reviewDockHeight = $state(loadDockHeight('stroke:review-dock-height', 220))
  /** rAF handle + pending height for the dock drag. */
  let _fkDockRafId = 0
  let _fkDockPendingH = 0
  // Active drag listeners (column resize / FK dock resize). Tracked so a mid-drag
  // unmount can remove them in onDestroy instead of leaking them on window.
  /** @type {{ move: (e: PointerEvent) => void, up: () => void } | null} */
  let _activeResizeListeners = null
  function clearActiveResizeListeners() {
    if (_fkDockRafId) { cancelAnimationFrame(_fkDockRafId); _fkDockRafId = 0 }
    if (!_activeResizeListeners) return
    window.removeEventListener('pointermove', _activeResizeListeners.move)
    window.removeEventListener('pointerup', _activeResizeListeners.up)
    _activeResizeListeners = null
  }

  /**
   * Drag-resize a bottom dock. Both docks are flex siblings of the scroll
   * container with the same constraints and the same reflow cost, so they share
   * the drag rather than keeping two copies of it.
   * @param {PointerEvent} e @param {'fk' | 'cell' | 'review'} which
   */
  function startDockResize(e, which) {
    e.preventDefault()
    clearActiveResizeListeners()
    const getH = () => (which === 'fk' ? fkDockHeight : which === 'cell' ? cellDockHeight : reviewDockHeight)
    const startY = e.clientY, startH = getH()
    const storageKey = `stroke:${which}-dock-height`
    const setH = (/** @type {number} */ h) => {
      if (which === 'fk') fkDockHeight = h
      else if (which === 'cell') cellDockHeight = h
      else reviewDockHeight = h
    }
    // Cleared per drag: a bare click with no movement would otherwise flush the
    // PREVIOUS drag's height on pointerup and make the dock jump.
    _fkDockPendingH = 0
    // Coalesce to one height write per frame. The dock is a flex sibling of the
    // scroll container, so every write reflows the grid, fires its ResizeObserver,
    // resizes the canvas backing store (which clears it) and forces a full
    // repaint. Writing that straight from pointermove ran the whole chain 120×/s
    // on a ProMotion trackpad - several times per painted frame - which is why
    // dragging the related-rows panel juddered and the grid flashed behind it.
    const move = (/** @type {PointerEvent} */ ev) => {
      _fkDockPendingH = Math.min(FK_DOCK_MAX, Math.max(FK_DOCK_MIN, startH + (startY - ev.clientY)))
      if (_fkDockRafId) return
      _fkDockRafId = requestAnimationFrame(() => {
        _fkDockRafId = 0
        setH(_fkDockPendingH)
      })
    }
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      _activeResizeListeners = null
      if (_fkDockRafId) { cancelAnimationFrame(_fkDockRafId); _fkDockRafId = 0 }
      // Land on the last position the pointer actually reached, not on whichever
      // frame happened to win the race with pointerup.
      if (_fkDockPendingH) setH(_fkDockPendingH)
      try { localStorage.setItem(storageKey, String(getH())) } catch {}
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    _activeResizeListeners = { move, up }
  }

  /** Column whose quick-stats panel is open, or null. */
  let statsCol = $state(/** @type {string | null} */ (null))

  /**
   * True when the grid shows a real table (schema/table known). Ad-hoc result
   * sets (SQL console, embedded AI results) have no table identity, so actions
   * that query or mutate the source table - column stats, filter by value/
   * column, edit, duplicate, delete - are hidden.
   */
  const hasTableContext = $derived(!!tableName)

  let contextRowIdx = $state(0);
  let contextColIdx = $state(0);
  let contextMenuOpen = $state(false);

  // Array cell editor (Prisma-style add/remove for SQL array columns).
  // Vector cells get their own viewer: a 1536-number literal in a textarea is
  // the value without any of the meaning.
  let vectorViewerOpen = $state(false);
  let vectorViewerRow = $state(0);
  let vectorViewerCol = $state(0);
  let vectorViewerColName = $state("");
  let vectorViewerType = $state("vector");
  let vectorViewerNullable = $state(false);
  /** The cell cannot be written (read-only session, no primary key) - preview only. */
  let vectorViewerReadOnly = $state(false);
  let vectorViewerValue = $state("");

  let geomViewerOpen = $state(false);
  let geomViewerRow = $state(0);
  let geomViewerCol = $state(0);
  let geomViewerColName = $state("");
  let geomViewerType = $state("geometry");
  let geomViewerNullable = $state(false);
  /** Same as the vector viewer: readable even where it is not writable. */
  let geomViewerReadOnly = $state(false);
  let geomViewerValue = $state("");

  let arrayEditorOpen = $state(false);
  let arrayEditorRow = $state(0);
  let arrayEditorCol = $state(0);
  let arrayEditorColName = $state("");
  let arrayEditorType = $state("");
  let arrayEditorValue = $state(/** @type {any[]} */ ([]));
  /** Block item activation from the right-click pointerup that opened the menu */
  let suppressMenuSelect = $state(false);
  /** Row indices with inline JSON detail open. Seeded from initialExpandedRows
   *  so background/snapshot panes render already-expanded (no collapse flicker). */
  //  Seeded ONCE on mount: untrack() says that to the compiler, which otherwise
  //  warns that only the initial value is captured (state_referenced_locally).
  let expandedRows = $state(untrack(() => new Set(initialExpandedRows ?? [])));
  /** @type {Record<string, number>} */
  let columnWidths = $state({});
  /** @type {string | null} */
  let resizingColName = $state(null);
  let resizeStartWidth = 0;

  // ── Virtual expression columns ───────────────────────────────────────────────
  /** Per-id logical widths (un-zoomed px) for virtual expr cols. */
  let _vexprWidths = $state(/** @type {Record<string,number>} */ ({}));

  // ── Keyboard navigation / undo ────────────────────────────────────────────
  /** Column name briefly highlighted after the user picks it from the toolbar's
   *  "Jump to column" menu (header + body band). null = no highlight. */
  let focusColName = $state(/** @type {string | null} */ (null));
  /** Column names selected via click / shift+click on column headers. */
  let selectedCols = $state(/** @type {Set<string>} */ (new Set()));
  /** Anchor column for shift+click range selection (plain var - not reactive). */
  let _lastHeaderClickedCol = /** @type {string | null} */ (null);

  /**
   * Rectangular cell-range selection (spreadsheet-style). The fixed corner is
   * `selAnchor`; the moving corner is `focusedRow`/`focusedCol` (visible-column
   * space). null = plain single-cell focus.
   *
   * NOTE: nothing currently sets this non-null - the shift+click / drag-select
   * sources were removed while the feature is parked. The draw/copy plumbing
   * (computeCellRange, copyCellRange, range tint in drawCell) is kept so the
   * feature can be re-enabled by adding a setter.
   * @type {{ row: number, col: number } | null}
   */
  let selAnchor = $state(null);

  /**
   * The active rectangular range in visible-column space, or null for a single
   * cell. A plain function (not $derived) because the canvas draw() reads it from
   * the rAF loop - outside any reactive context - where reading a $derived would
   * trip Svelte's `derived_inert` warning and return stale values. It only reads
   * $state (safe to read anywhere) and stays in sync automatically.
   * @returns {{ r0: number, r1: number, c0: number, c1: number } | null}
   */
  function computeCellRange() {
    if (selAnchor === null || focusedRow === null || focusedCol === null) return null;
    const r0 = Math.min(selAnchor.row, focusedRow);
    const r1 = Math.max(selAnchor.row, focusedRow);
    const c0 = Math.min(selAnchor.col, focusedCol);
    const c1 = Math.max(selAnchor.col, focusedCol);
    if (r0 === r1 && c0 === c1) return null; // collapsed to one cell
    return { r0, r1, c0, c1 };
  }

  /** Extend column selection from anchor to `toColName`, using geom.cols order. */
  function extendColSelection(toColName) {
    if (!_lastHeaderClickedCol) { selectedCols = new Set([toColName]); _lastHeaderClickedCol = toColName; return }
    const startIdx = geom.cols.findIndex((c) => c.name === _lastHeaderClickedCol)
    const endIdx = geom.cols.findIndex((c) => c.name === toColName)
    if (startIdx < 0 || endIdx < 0) return
    const [lo, hi] = [Math.min(startIdx, endIdx), Math.max(startIdx, endIdx)]
    selectedCols = new Set(geom.cols.slice(lo, hi + 1).map((c) => c.name))
  }
  /** @type {ReturnType<typeof setTimeout> | null} */
  let _focusColTimer = null;
  /** Scrollable container element for programmatic focus + scroll. */
  let tableContainer = $state(/** @type {HTMLDivElement | null} */ (null));
  /** Whether to select-all text when the edit input is focused. */
  let selectOnEditFocus = $state(true);
  /** Whether the enum cell-editor dropdown is open (auto-opens on edit). */
  let enumEditorOpen = $state(false);
  /** Set by the enum menu's own onValueChange so the close that FOLLOWS a pick is
   *  not mistaken for a dismissal. commitEdit() is async, so editingCell is still
   *  set at the moment the menu closes and the watcher below cannot tell the two
   *  apart without this. */
  let _enumPicked = false;
  // Closing the enum menu without picking cancels the edit - the contract the
  // bits-ui Select's onOpenChange used to carry. SelectMenu exposes `open` as a
  // binding rather than a callback, so it is watched instead.
  $effect(() => {
    const open = enumEditorOpen;
    untrack(() => {
      if (open) { _enumPicked = false; return }
      if (_enumPicked) { _enumPicked = false; return }
      if (editingCell) cancelEdit();
    });
  });
  /** Raw cell value before the current edit started (for undo tracking). */
  let lastEditOriginalValue = $state(/** @type {unknown} */ (undefined));
  /**
   * Committed edit history for Ctrl+Z.
   * @type {{ rowIdx: number, colIdx: number, oldValue: unknown, newValue: unknown }[]}
   */
  let pastEdits = $state([]);
  /**
   * Undone edits available for Ctrl+Shift+Z / Ctrl+Y.
   * @type {{ rowIdx: number, colIdx: number, oldValue: unknown, newValue: unknown }[]}
   */
  let futureEdits = $state([]);
  /** True while focus is inside this table (container or any child). */
  let isTableFocused = $state(false);

  /** Draft values for the pending new row, keyed by column name. null = no new row. */
  /**
   * Rows staged for insert, oldest first, rendered as a band under the header.
   *
   * A list rather than one row: duplicating twice means two new rows, and
   * filling in three related records should not be three round trips through
   * Add → type → Insert. `null` and `[]` both mean "no band".
   * @type {Record<string, string>[] | null}
   */
  let newRowDrafts = $state(/** @type {Record<string, string>[] | null} */ (null))
  /** Name of the column whose input is focused in the new row. */
  let newRowFocusCol = $state(/** @type {string | null} */ (null))
  /** Which staged row the focused field belongs to. */
  let newRowFocusIdx = $state(0)


  // ── Canvas zoom ────────────────────────────────────────────────────────────
  // canvasZoom mirrors the app zoom: applySettings() writes the app zoom into the
  // shared zoomState store, so all open tabs zoom together with the rest of the UI
  // and the level persists via app settings.

  // All layout constants scale with canvasZoom so the entire canvas zooms together.
  // The base height comes from Settings → Appearance → Row spacing; everything
  // else in the grid (row tops, hit tests, the scroll spacer) is derived from
  // ROW_HEIGHT, so changing it reflows the whole grid with no other edits.
  const ROW_HEIGHT = $derived(Math.round(rowSpacingHeight($appRowSpacing) * canvasZoom))

  // `_scrollTop` is the VIRTUAL scroll offset used for all row math (content
  // space). `_physScrollTop` is the raw DOM scrollTop of the container, which is
  // bounded by the capped spacer. They're equal until a table is tall enough to
  // exceed the browser's element-height ceiling (see MAX_SCROLL_PX), at which
  // point the physical range is compressed and _scrollTop = _physScrollTop×scale.
  let _scrollTop = $state(0)
  let _physScrollTop = $state(0)
  // Start high so the first paint covers any reasonable screen height before the
  // ResizeObserver fires with the real value.
  let _viewportHeight = $state(1200)

  // ── Canvas rendering ──────────────────────────────────────────────────────
  const HEADER_H = $derived(Math.round(30 * canvasZoom))
  const GUTTER_EXPAND_W = $derived(Math.round(32 * canvasZoom))
  const GUTTER_SELECT_W = $derived(Math.round(36 * canvasZoom))
  /**
   * Row-number gutter, sized to the widest number it will actually draw. A fixed
   * width either wastes space on a 50-row page or clips at a million rows, and
   * this is measured from the page's last number, not from the total.
   */
  const GUTTER_NUM_W = $derived(
    $appRowNumbers
      // 8px per digit: the numbers are drawn at the cell size now, and a mono
      // digit is ~0.6em, so a 13px glyph needs the wider allowance or a
      // six-figure row number clips against the first column's rule.
      ? Math.round((String(Math.max(1, rowNumberOffset + rows.length)).length * 8 + 16) * canvasZoom)
      : 0,
  )
  /** @type {HTMLCanvasElement | null} */
  let canvasEl = $state(null)
  /** @type {HTMLSpanElement | null} */
  let colorProbe = $state(null)
  let _scrollLeft = $state(0)
  let _viewportWidth = $state(800)
  /** Bumped to force a repaint when a non-reactive input (theme) changes. */
  let _redrawToken = $state(0)
  /** Hover target (drawn affordances + tooltip + cursor). */
  let hoveredRow = $state(/** @type {number | null} */ (null))
  let hoveredColName = $state(/** @type {string | null} */ (null))
  /** Column whose right edge the pointer is over in the header (resize cursor). */
  let _resizeHoverCol = $state(/** @type {string | null} */ (null))
  /** Swallow the click that fires right after a resize-drag pointerup. */
  let _suppressNextClick = false
  /** Right-click target kind, so one ContextMenu can show header vs body items. */
  let contextIsHeader = $state(false)
  let contextHeaderCol = $state("")

  // Map a visible/any column name → its index in the full `columns` array
  // (rows[] are indexed by the full column order, including hidden columns).
  const _nameToActualIdx = $derived.by(() => {
    /** @type {Map<string, number>} */
    const m = new Map()
    columns.forEach((c, i) => m.set(c.name, i))
    return m
  })

  /** Extra body offset for the inline insert-row slot (a DOM overlay). */
  const draftCount = $derived(newRowDrafts?.length ?? 0)
  const insertRowOffset = $derived(draftCount * ROW_HEIGHT)
  /** Measured heights for each expanded row (rowIdx → px). Updated by ResizeObserver. */
  let expandedRowHeights = $state(/** @type {Map<number, number>} */ (new Map()))

  /**
   * Svelte action: observes an expand panel's rendered height and updates the map.
   * Cleans up when the panel unmounts (row collapsed).
   * @param {HTMLElement} node
   * @param {number} rowIdx
   */
  function trackExpandHeight(node, rowIdx) {
    // Debounce height updates so rapid changes (toolbar → loaded content) collapse
    // into a single reactive write, avoiding a double layout-shift.
    let timer = 0
    const commit = () => {
      const h = node.offsetHeight
      // Ignore tiny heights that just reflect the loading/toolbar-only state -
      // keep the current allocation until real content settles.
      if (h >= 40 && expandedRowHeights.get(rowIdx) !== h) {
        expandedRowHeights = new Map(expandedRowHeights).set(rowIdx, h)
      }
    }
    // Measure synchronously on mount (forces one layout) so rowTops uses the real
    // panel height on the first paint - the row below never flashes at the 280px
    // placeholder and then snaps up. The observer below handles later size changes.
    commit()
    const ro = new ResizeObserver(() => { clearTimeout(timer); timer = setTimeout(commit, 40) })
    ro.observe(node)
    return {
      destroy() {
        clearTimeout(timer)
        ro.disconnect()
        // In scaled/huge mode the panel unmounts whenever the row scrolls out of the
        // near-viewport render window, even though the row is STILL expanded. Forgetting
        // its height then shrinks contentHeight → shifts _scrollScale → shifts every
        // row's virtual position → oscillation: the expanded row bounces in and out of
        // the render window (intermittent black-out of the JSON panel) and the bottom
        // drifts out of reach. Only forget the height on a real collapse; a scroll-out
        // keeps the last measured height so the reservation stays rock-steady.
        if (expandedRows.has(rowIdx)) return
        const next = new Map(expandedRowHeights)
        next.delete(rowIdx)
        expandedRowHeights = next
      },
    }
  }

  // ── Lightbox (click-to-open image / PDF) ──────────────────────────────────
  /** @type {string | null} */
  let lightboxUrl = $state(null);

  /** @type {'image' | 'pdf'} */
  let lightboxType = $state("image");

  async function openExternal(/** @type {string} */ url) {
    try {
      const isTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (isTauri) {
        const { openUrl: open } = await import("@tauri-apps/plugin-opener");
        await open(url);
      } else {
        window.open(url, "_blank", "noopener,noreferrer");
      }
    } catch (err) {
      toast.error(`Could not open URL: ${String(err)}`);
    }
  }

  function canEditColumn(colIdx) {
    if (readonly) return false;
    const col = columns[colIdx];
    if (!col || !primaryKey.length) return false;
    return isEditableType(col.dataType ?? col.data_type ?? "");
  }

  const menuColName = $derived(columns[contextColIdx]?.name ?? "cell");
  const menuForeignKey = $derived(
    menuColName ? findForeignKeyForColumn(foreignKeys, menuColName) : null,
  );
  const menuEditable = $derived(canEditColumn(contextColIdx));
  const menuColPinned = $derived(pinnedColumns.has(menuColName));
  /**
   * NOT NULL column → no "Set NULL". The server would reject the write, so the
   * item was an action that could only ever fail; it is dropped rather than
   * disabled, because a disabled row still says "this is a thing you might do
   * here" and on a NOT NULL column it never is.
   */
  const menuColNullable = $derived(columns[contextColIdx]?.nullable !== false);
  const menuCellNull = $derived(
    rows[contextRowIdx]?.[contextColIdx] === null ||
      rows[contextRowIdx]?.[contextColIdx] === undefined,
  );
  // Truncated cells only hold a preview - filtering on it would build wrong SQL.
  const menuCellOversize = $derived(!!oversizeCellInfo(rows[contextRowIdx]?.[contextColIdx]));
  // Context-aware quick filters for the right-clicked cell. Only scans the loaded
  // rows for categorical/distinct values when the page is modest - large tables
  // (1M+ rows are a supported use case) skip the scan and fall back to metadata.
  const quickFilter = $derived.by(() => {
    if (!hasTableContext || contextColIdx < 0 || contextRowIdx < 0) return null;
    const col = columns[contextColIdx];
    if (!col || menuCellOversize) return null;
    const cellValue = rows[contextRowIdx]?.[contextColIdx];
    const colValues = rows.length <= 2000 ? rows.map((r) => r?.[contextColIdx]) : [];
    return buildQuickFilter(col, cellValue, colValues);
  });
  // SQL array column? (value already decoded to a JS array, or type ends with []).
  const menuColType = $derived(
    String(columns[contextColIdx]?.dataType ?? columns[contextColIdx]?.data_type ?? _colCache[contextColIdx]?.colType ?? ""),
  );
  // The dedicated array editor writes a Postgres array literal ({a,b}) cast to the
  // real array type - that's native to PostgreSQL & CockroachDB (Neon/Supabase/
  // Prisma all speak the pg wire protocol, so they route through the same path).
  // Other engines either have no native arrays (MySQL/SQLite/MSSQL) or use a
  // different literal (ClickHouse/DuckDB [..]), so restrict the editor to pg-family
  // to avoid producing a write the backend can't apply.
  const isPgArrayDialect = $derived(dialect === "postgres" || dialect === "cockroachdb");
  const menuCellIsArray = $derived(
    isPgArrayDialect &&
      (Array.isArray(rows[contextRowIdx]?.[contextColIdx]) || /\[\]\s*$/.test(menuColType)),
  );
  // Extension-provided transforms applicable to the right-clicked cell.
  const menuTransforms = $derived.by(() => {
    void $pluginState;
    const v = rows[contextRowIdx]?.[contextColIdx];
    if (v === null || v === undefined) return [];
    // Column-only transforms (avatar / image thumbnail) render live on a whole
    // column; they're offered in the header menu, not the per-cell menu.
    return transformsFor(v, _colCache[contextColIdx]?.colType ?? "", menuColName).filter((tf) => !tf.columnOnly);
  });
  // Value generators (UUIDv7, nanoid, …) offered when the cell is editable.
  const menuGenerators = $derived.by(() => {
    void $pluginState;
    return enabledGeneratorGroups();
  });
  // Transforms offered for a whole column (header menu), decided from a sample
  // of the column's first non-null value.
  const menuColTransforms = $derived.by(() => {
    void $pluginState;
    const name = contextHeaderCol;
    if (!name) return [];
    const actualIdx = _nameToActualIdx.get(name) ?? -1;
    if (actualIdx < 0) return [];
    let sample = null;
    for (let i = 0; i < rows.length && i < 200; i++) {
      const v = rows[i]?.[actualIdx];
      if (v !== null && v !== undefined) { sample = v; break }
    }
    if (sample === null) return [];
    return transformsFor(sample, _colCache[actualIdx]?.colType ?? "", name);
  });

  const CELL_DISPLAY_LIMIT = 400

  // Cache stringified object/array cells - row values are stable references
  // until a refetch, so we stringify each once instead of on every re-render
  // (focus/selection/scroll all re-evaluate visible cells).
  /** @type {WeakMap<object, string>} */
  const _formatCache = new WeakMap();
  // Grouped-number formatter, rebuilt only when the setting flips. Intl is slow
  // enough to matter when a screen of cells each formats on every frame.
  let _groupFmt = /** @type {Intl.NumberFormat | null} */ (null)
  function groupNumber(/** @type {number} */ n) {
    // Integers only. A float would be rounded to Intl's default 3 decimals, and
    // silently changing a value the user is reading off the database is worse
    // than leaving it ungrouped. Beyond 2^53 the digits are already unreliable,
    // and bigints arrive as strings anyway.
    if (!Number.isSafeInteger(n)) return String(n)
    _groupFmt ??= new Intl.NumberFormat()
    return _groupFmt.format(n)
  }
  // Hot-path mirrors. `$store` compiles to a store_get() call, and formatCell and
  // drawCell each run once per visible CELL per frame - so a store read there is
  // thousands of calls a frame, at 120Hz. Mirrored into plain locals so the hot
  // path reads a variable, which is what the frame context above does for
  // $appTableAlign and the table style.
  let _numberGroupingOn = false
  let _imagePreviewOn = true
  $effect(() => { _numberGroupingOn = $appNumberGrouping; _groupFmt = null; scheduleDraw() })
  $effect(() => { void $appHighlightActiveRow; scheduleDraw() })
  // Turning previews off must also drop what was already decoded - otherwise the
  // thumbnails stay resident (up to 300 bitmaps) for a feature now switched off.
  $effect(() => {
    const on = $appImagePreview
    _imagePreviewOn = on
    untrack(() => { if (!on) releaseCellImages(); _redrawToken++; scheduleDraw() })
  })

  function formatCell(value) {
    // NULL stays the literal "NULL" here. drawCell swaps it for ∅ when the
    // "Empty & NULL Markers" extension is on - that extension is the NULL
    // display control, and a setting competing with it would be a second switch.
    if (value === null || value === undefined) return "NULL";
    if (typeof value === "number" && _numberGroupingOn) return groupNumber(value);
    if (typeof value === "object") {
      const cached = _formatCache.get(value);
      if (cached !== undefined) return cached;
      // Oversize sentinels carry a preview instead of the real (multi-MB)
      // value - render the truncation marker + head, not the sentinel wrapper.
      const over = oversizeCellInfo(value);
      // JSON/JSONB objects and arrays render as JSON here. SQL *array columns* get
      // the pgAdmin {a,b} form instead, but that decision needs the column type, so
      // it lives in drawCell (arrayDisplay) - a jsonb array must stay ["a","b"].
      const s = over ? oversizeCellText(over) : JSON.stringify(value);
      _formatCache.set(value, s);
      return s;
    }
    return String(value);
  }

  // Render a JS array as a Postgres array literal for display: {a,b}, {} for
  // empty, NULL for null elements. Elements are quoted only when they contain a
  // delimiter/quote/brace/whitespace or would be ambiguous - matching pgAdmin.
  function pgArrayElem(el) {
    if (el === null || el === undefined) return "NULL";
    // Nested arrays (multi-dim) recurse; objects (e.g. json[]) fall back to JSON.
    if (Array.isArray(el)) return pgArrayText(el);
    if (typeof el === "object") return JSON.stringify(el);
    const s = String(el);
    if (s === "" || /[",{}\\\s]/.test(s) || /^null$/i.test(s)) {
      return '"' + s.replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"';
    }
    return s;
  }
  function pgArrayText(arr) {
    return "{" + arr.map(pgArrayElem).join(",") + "}";
  }
  // Display for SQL *array columns* (drawCell passes the value after confirming
  // the column type ends with []). Cached per value object so the scroll hot
  // path never rebuilds the string.
  //
  // JSON form, not the pgAdmin literal `{a,b}`. Three things in this app showed
  // the same array three different ways: a text[] cell read `{Dhaka,Gazipur}`,
  // the jsonb column beside it read `["Dhaka","Gazipur"]`, and double-clicking
  // either one put `["Dhaka","Gazipur"]` in the box to edit - so the row you
  // were reading and the value you were editing did not look like the same
  // thing. They all read as JSON now, which is the form the editor already used.
  // pgArrayText is still what writes go out as; that is the literal Postgres
  // wants and it was never the right thing to read.
  /** @type {WeakMap<object, string>} */
  const _arrayDisplayCache = new WeakMap();
  function arrayDisplay(arr) {
    const hit = _arrayDisplayCache.get(arr);
    if (hit !== undefined) return hit;
    let s;
    try {
      s = JSON.stringify(arr);
    } catch {
      s = pgArrayText(arr); // cyclic or otherwise unserialisable - fall back
    }
    _arrayDisplayCache.set(arr, s ?? pgArrayText(arr));
    return s ?? pgArrayText(arr);
  }
  /** True when a column's SQL type is an array (ends with []). */
  function isSqlArrayType(colType) {
    return /\[\]\s*$/.test(colType ?? "");
  }

  /** pgvector column types, whose values arrive as `[0.1,0.2,…]` text. */
  function isVectorType(colType) {
    return /^(vector|halfvec|sparsevec)\b/i.test(String(colType ?? "").trim());
  }

  // An embedding printed in full fills the row with digits that say nothing at a
  // glance. The dimension leads, then as much of the head as fits. Display only:
  // editing, copy and the viewer all still see the real value.
  /** @type {Map<string, { heads: number, out: string }>} */
  const _vectorDisplayCache = new Map();
  /**
   * How many leading values to show, from the room the column actually has.
   * A narrow column that renders `384d · 0.1, 0.1, 0.…` is worse than one that
   * renders `384d`: the dimension is the part you can read, and half a number is
   * noise. So the head count follows the width instead of being fixed at three.
   * @param {number} cellW
   */
  function vectorHeads(cellW) {
    const glyph = _glyphW > 0 ? _glyphW : Math.max(6, (_fonts?.cellPx ?? 13) * 0.6);
    const fits = Math.floor((cellW - CELL_PAD_X * 2) / glyph);
    // "384d · " is ~7 glyphs and each value with its separator is ~7 more.
    if (fits < 14) return 0;
    return Math.max(1, Math.min(6, Math.floor((fits - 7) / 7)));
  }
  /** @param {string} text @param {number} heads */
  function vectorDisplay(text, heads) {
    const hit = _vectorDisplayCache.get(text);
    if (hit !== undefined && hit.heads === heads) return hit.out;
    const out = vectorSummary(text, heads);
    if (_vectorDisplayCache.size > 4096) _vectorDisplayCache.clear();
    _vectorDisplayCache.set(text, { heads, out });
    return out;
  }

  /** Truncated version for DOM rendering - keeps long values out of the render tree */
  // Characters with no glyph anywhere: the C0 and C1 control ranges and DEL.
  // Tab, newline and carriage return are left out - they are ordinary in text
  // columns and escaping them would rewrite every multi-line value on screen.
  const CONTROL_CHARS = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F]/;
  const CONTROL_CHARS_G = new RegExp(CONTROL_CHARS.source, "g");

  /**
   * Show control characters instead of drawing nothing where they are.
   *
   * A font has no glyph for these, so the grid drew each one as a blank box and
   * a value carrying one was indistinguishable from a value that did not - the
   * mojibake `â\u0080¯` read as `â ¯` with a hole in the middle, and nothing on
   * screen said what the hole was.
   *
   * Escaped in ASCII rather than swapped for a Control Pictures glyph (␀): the
   * replacement has to be certain to render, and those glyphs are missing from
   * plenty of monospace faces - which would put the box straight back.
   */
  function showControlChars(/** @type {string} */ s) {
    if (!CONTROL_CHARS.test(s)) return s;
    return s.replace(CONTROL_CHARS_G, (c) =>
      "\\u" + (c.codePointAt(0) ?? 0).toString(16).padStart(4, "0"),
    );
  }

  /**
   * Fold a multi-line value onto the one line a grid row has for it.
   *
   * fillText draws no line breaks, so a newline came out as nothing at all
   * while the indentation around it was drawn in full - pretty-printed JSON
   * read as `[   "a",   "b" ]`, gaps where the structure used to be. The break
   * and the whitespace either side of it collapse to a single space, which is
   * what the copy-as-TSV path already does with the same values.
   */
  function foldLines(/** @type {string} */ s) {
    return s.includes("\n") || s.includes("\r") ? s.replace(/\s*[\r\n]+\s*/g, " ") : s;
  }

  function displayCell(value) {
    // Escaped before the cut, so the limit counts what is actually drawn and an
    // escape can never be sliced in half.
    const s = foldLines(showControlChars(formatCell(value)));
    return s.length > CELL_DISPLAY_LIMIT ? s.slice(0, CELL_DISPLAY_LIMIT) + "…" : s;
  }

  // Per-row display-string caches for the draw hot path, keyed by ROW INDEX and
  // tied to the current `rows` identity (syncDisplayCaches drops them whenever the
  // data changes - every write path replaces `rows`). Capped in size so scrolling
  // through millions of rows can't retain a string[] for every row ever seen; the
  // old WeakMap-by-row versions never released because every loaded row stays
  // alive in the `rows` prop. Staged edits bypass these (their value differs).
  const _DISP_CACHE_MAX = 8192;
  /** @type {Map<number, string[]>} */
  const _cellTextCache = new Map();
  /** @type {Map<number, string[]>} */
  const _vexprTextCache = new Map();
  /** @type {Map<number, Map<string, string>>} */
  const _colTfCache = new Map();
  /** The row object each cached index was built from. The array-identity check
   *  below misses one case: saving a cell swaps that ONE row for a new array and
   *  leaves `rows` itself the same object (it is $state.raw - the parent bumps
   *  dataVersion instead of replacing it), so the caches kept serving the
   *  pre-edit text and an applied edit only appeared after a refresh.
   *  @type {Map<number, unknown>} */
  const _dispCacheRowRef = new Map();
  let _dispCacheRows = /** @type {unknown} */ (null);
  let _dispCacheVFns = /** @type {unknown} */ (null);
  let _dispCacheTFns = /** @type {unknown} */ (null);
  /** Invalidate cached cell text when the data or active virtual/transform fns
   *  change. Called once per frame from draw(); all draw-path callers key by index. */
  function syncDisplayCaches() {
    if (_dispCacheRows !== rows) {
      _dispCacheRows = rows;
      _cellTextCache.clear(); _vexprTextCache.clear(); _colTfCache.clear();
      _dispCacheRowRef.clear();
      // Keyed by the full vector literal (10-20KB each for big embeddings), so
      // stale entries from previous pages/tables must not accumulate.
      _vectorDisplayCache.clear();
    }
    if (_dispCacheVFns !== _vcolFns) { _dispCacheVFns = _vcolFns; _vexprTextCache.clear(); }
    if (_dispCacheTFns !== _colTransformFns) { _dispCacheTFns = _colTransformFns; _colTfCache.clear(); }
    // The truncation cache is keyed by (font, width, string) and stays correct
    // across pages, so it deliberately survives a `rows` swap - that is what
    // keeps windowed scrolling on cache hits. It is dropped on a column change
    // so one table's strings are not held alive after you navigate to another.
    if (_truncCacheCols !== columns) { _truncCacheCols = columns; _truncCacheReset(); }
  }
  let _truncCacheCols = /** @type {unknown} */ (null);
  /** Drop one row's cached text when the row behind it was replaced in place.
   *  Called once per row per frame from drawBodyRow - not per cell, which would
   *  put a Map lookup on every drawn cell for a check that can only change per row. */
  function syncRowDisplayCache(/** @type {number} */ idx) {
    const row = rows[idx];
    if (_dispCacheRowRef.get(idx) === row) return;
    if (_dispCacheRowRef.size >= _DISP_CACHE_MAX) _dispCacheRowRef.clear();
    _dispCacheRowRef.set(idx, row);
    _cellTextCache.delete(idx);
    _vexprTextCache.delete(idx);
    _colTfCache.delete(idx);
  }
  function cellDisplayText(/** @type {number} */ idx, /** @type {number} */ actualIdx, /** @type {unknown} */ value) {
    let arr = _cellTextCache.get(idx);
    if (!arr) {
      if (_cellTextCache.size >= _DISP_CACHE_MAX) _cellTextCache.clear();
      arr = []; _cellTextCache.set(idx, arr);
    }
    const hit = arr[actualIdx];
    if (hit !== undefined) return hit;
    const s = displayCell(value);
    arr[actualIdx] = s;
    return s;
  }
  // Virtual expression columns - the bound evaluator would otherwise run per cell
  // per frame (string building on every scroll frame).
  function vexprText(/** @type {number} */ idx, /** @type {number} */ fnIdx) {
    let arr = _vexprTextCache.get(idx);
    if (!arr) {
      if (_vexprTextCache.size >= _DISP_CACHE_MAX) _vexprTextCache.clear();
      arr = []; _vexprTextCache.set(idx, arr);
    }
    let s = arr[fnIdx];
    if (s === undefined) {
      const fn = _vcolFns[fnIdx];
      s = fn ? fn(rows[idx]) : '';
      arr[fnIdx] = s;
    }
    return s;
  }

  // Whether any formatter/linkifier is enabled - gates the per-cell directive
  // lookup so the scroll hot path does zero extension work in the common case.
  const _extActive = $derived.by(() => { void $pluginState; void $externalFormatVersion; return anyDisplayExtEnabled(); });
  // Reused per-cell stats context - formatters read `.stats` synchronously and
  // don't retain it, so one scratch object avoids an allocation per drawn cell
  // while a stats-dependent extension (heatmap / annotator) is enabled.
  const _statsCtx = { stats: /** @type {any} */ (undefined) };

  // Windowed mode: repaint when the parent splices in a freshly-loaded window
  // (dataVersion bumps without a rows-identity change), and report the visible
  // range so the parent can fetch/evict windows. Emitting only on change keeps
  // this off the per-frame hot path.
  $effect(() => { void dataVersion; scheduleDraw(); });
  let _emittedFirst = -1;
  let _emittedLast = -1;
  // A reload that keeps the scroll position hands over a *fresh* rows array -
  // for a windowed view, one holding nothing but its probe. The viewport has not
  // moved, so the range is the same one the parent already heard and the dedupe
  // below would swallow the emit, leaving every row under the viewport a
  // skeleton until the user happened to scroll. Identity change = forget what
  // was emitted, and let the next frame re-ask for the windows that matter.
  $effect(() => {
    void rows;
    _emittedFirst = -1;
    _emittedLast = -1;
  });
  function emitVisibleRange(first, last) {
    if (!windowed) return;
    if (first === _emittedFirst && last === _emittedLast) return;
    _emittedFirst = first;
    _emittedLast = last;
    onvisiblerange(first, last);
  }

  // Repaint when extension settings or column stats change - both affect drawn
  // cell text, badges, tints, and the header annotator strip.
  // `externalFormatVersion` is the other half of the external-plugin contract:
  // a plugin's directives arrive after the frame that asked for them, and this
  // is what brings that frame back.
  $effect(() => { void $pluginState; void $externalFormatVersion; void _colStats; scheduleDraw(); });

  // Resolved canvas-grid style preset (Settings → Appearance). Read once per frame
  // by draw() and passed into the row context, so it never adds per-cell reactivity.
  // The zebra flag is folded in here rather than checked per cell: the preset can
  // bring its own alternating shading (Striped, Dots) and the standalone setting
  // adds it to any other preset, so draw() only ever reads one boolean.
  const _tableStyle = $derived.by(() => {
    const preset = TABLE_STYLES[normalizeTableStyle($appTableStyle)]
    const zebra = preset.zebra === true || $appZebraRows === true
    return zebra === (preset.zebra === true) ? preset : { ...preset, zebra }
  });
  // Repaint the grid the moment the user switches preset, alignment, spacing or shading.
  $effect(() => { void $appTableStyle; scheduleDraw(); });
  $effect(() => { void $appTableAlign; scheduleDraw(); });
  $effect(() => { void $appZebraRows; scheduleDraw(); });
  // Row spacing had no redraw of its own, so changing it moved the GEOMETRY
  // without repainting: `contentHeight` and `spacerHeight` are $derived, so the
  // scrollbar resized immediately while the canvas kept the pixels it had drawn
  // at the old row height. The rows only took the new size once something else
  // happened to trigger a paint - a scroll, a hover, a click - which reads as
  // "row spacing half works". Every other grid-appearance store has this line;
  // this one was missed when the setting was added.
  // `_redrawToken++` READS and WRITES `_redrawToken`, which is `$state`. Done in
  // a tracked effect that is an infinite loop - the write re-triggers the read -
  // and Svelte kills the view with effect_update_depth_exceeded. `untrack` is how
  // every other writer here does it (see the zoom watcher below); the row-spacing
  // dependency stays tracked because `$appRowSpacing` is read outside it.
  $effect(() => {
    void $appRowSpacing
    untrack(() => { _redrawToken++; scheduleDraw() })
  });

  // ── Search-match highlighting ──────────────────────────────────────────────
  // The toolbar search filters rows server-side (ILIKE, case-insensitive);
  // this paints where each match falls inside the visible cell text. Matching
  // runs only while a search is active, on the already-truncated display
  // string, so the scroll hot path stays free of extra work otherwise.
  /**
   * The matcher the highlight paints with, built from the same query and the
   * same options the rows were fetched under.
   *
   * It used to be `indexOf` on a lowercased haystack, which contradicted the
   * result it was drawn on: with match-case on, a search for `aarav` returned
   * nothing containing `Aarav` and then highlighted `Aarav` anyway, and a regex
   * search highlighted the pattern's literal characters. Null when there is
   * nothing to paint, which keeps the scroll path free of this entirely.
   * @type {RegExp | null}
   */
  const _searchMatcher = $derived.by(() => {
    const q = String(searchQuery ?? '').trim();
    if (!q) return null;
    const flags = `g${searchOptions?.matchCase ? '' : 'i'}`;
    const body = searchOptions?.regex ? q : q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const pattern = searchOptions?.wholeWord ? `\\b(?:${body})\\b` : body;
    try {
      return new RegExp(pattern, flags);
    } catch {
      // A half-typed regex is not an error here - it just has nothing to mark.
      return null;
    }
  });
  $effect(() => { void _searchMatcher; scheduleDraw(); });

  const MAX_CELL_MATCH_HIGHLIGHTS = 8;
  /**
   * @param {CanvasRenderingContext2D} ctx @param {string} drawn
   * @param {number} textX @param {number} ry @param {number} rh @param {any} c
   */
  function drawSearchHighlights(ctx, drawn, textX, ry, rh, c) {
    const re = _searchMatcher;
    if (!re) return;
    const hh = Math.min(rh - 4, Math.round(17 * canvasZoom));
    const hy = ry + (rh - hh) / 2;
    ctx.fillStyle = withAlpha(c.AMBER, 0.3);
    // `lastIndex` survives a call on a /g/ regex, and this instance is shared by
    // every cell on screen - resetting it per cell is what stops the second cell
    // in a row from being searched from the first one's offset.
    re.lastIndex = 0;
    let n = 0;
    /** @type {RegExpExecArray | null} */
    let m;
    while (n < MAX_CELL_MATCH_HIGHLIGHTS && (m = re.exec(drawn)) !== null) {
      // A pattern that can match nothing (`a*`) would otherwise spin here.
      if (m[0] === '') { re.lastIndex++; continue; }
      const at = m.index;
      const x0 = textX + (at > 0 ? ctx.measureText(drawn.slice(0, at)).width : 0);
      const mw = ctx.measureText(m[0]).width;
      roundRect(ctx, x0 - 1, hy, mw + 2, hh, 3);
      ctx.fill();
      n++;
    }
  }

  /** True while a pointer press is what is moving focus into the grid. */
  let _focusFromPointer = false

  function focusRow(rowIdx) {
    if (editingCell) return;
    focusedRow = rowIdx;
    if (focusedCol === null) focusedCol = 0;
  }

  /** Maps a navigable-column index → actual column index in `columns`. Skips collapsed strips. */
  function visToActualColIdx(visColIdx) {
    const colName = navigableColumns[visColIdx]?.name;
    return colName ? columns.findIndex((c) => c.name === colName) : -1;
  }

  /** Maps an actual column index → navigable-column index (-1 if hidden/collapsed). */
  function actualToVisColIdx(actualColIdx) {
    const colName = columns[actualColIdx]?.name;
    return colName ? navigableColumns.findIndex((c) => c.name === colName) : -1;
  }

  /** @param {number} rowIdx */
  function scrollRowIntoView(rowIdx) {
    if (!tableContainer) return
    const top = rowDocTop(rowIdx)
    const bottom = top + ROW_HEIGHT
    const ch = tableContainer.clientHeight
    const vst = _scrollTop // current virtual offset
    // Keep the row clear of the pinned header band at the top of the viewport.
    if (top - HEADER_H < vst) {
      setVirtualScroll(top - HEADER_H)
    } else if (bottom > vst + ch) {
      setVirtualScroll(bottom - ch)
    }
  }

  /**
   * Bring a row into view AFTER the related-rows dock has taken its space.
   *
   * Scrolling in the same tick reads the pre-dock `clientHeight`, so a row near
   * the bottom is judged "already visible" and left alone - then the dock opens
   * over it, and the row you clicked to inspect is the one row you can no longer
   * see. `tick()` lets the dock lay out first; `clientHeight` is then the real,
   * shortened viewport.
   * @param {number} rowIdx
   */
  function scrollRowIntoViewBesideDock(rowIdx) {
    tick().then(() => scrollRowIntoView(rowIdx))
  }

  /**
   * Bring a column into view if it's off-screen. Pinned columns are always
   * visible, so they only ever need the highlight.
   * @param {string} name
   * @returns {boolean} false if there is no such column
   */
  function scrollColumnIntoView(name, behavior = "smooth") {
    // Relationship columns live right of the real ones, outside `geom`, so they
    // carry their own x/w - without this the cursor could Tab onto one that is
    // off-screen and the grid would sit still.
    const vp = _vrelLayout.find((p) => p.hoverKey === name)
    if (vp) {
      if (!tableContainer) return false
      const PAD = 28
      const vLeft = vp.x - _scrollLeft
      const vRight = vLeft + vp.w
      let target = _scrollLeft
      if (vLeft < geom.frozenWidth + PAD) target = vp.x - geom.frozenWidth - PAD
      else if (vRight > _viewportWidth - PAD) target = vp.x + vp.w - _viewportWidth + PAD
      target = Math.max(0, target)
      if (Math.abs(target - _scrollLeft) > 1) tableContainer.scrollTo({ left: target, behavior })
      return true
    }
    const col = geom.cols.find((c) => c.name === name)
    if (!col) return false
    if (tableContainer && !col.pinned) {
      // Left edge of the scrollable area is occluded by the frozen pinned cols.
      const frozen = geom.frozenWidth
      const PAD = 28
      const vLeft = col.contentX - _scrollLeft
      const vRight = vLeft + col.w
      let target = _scrollLeft
      if (vLeft < frozen + PAD) {
        target = col.contentX - frozen - PAD
      } else if (vRight > _viewportWidth - PAD) {
        target = col.contentX + col.w - _viewportWidth + PAD
      }
      target = Math.max(0, target)
      if (Math.abs(target - _scrollLeft) > 1) {
        tableContainer.scrollTo({ left: target, behavior })
      }
    }
    return true
  }

  /**
   * Did the cursor's last move come from the keyboard?
   *
   * Only a keyboard move should scroll. Tab and the arrow keys can walk the
   * cursor past the edge of the viewport, so the grid has to follow it or it
   * sits there claiming to be on a column that is off screen. A click cannot:
   * the cell was under the pointer, so it was already visible - and scrolling
   * it "into view" then yanked the grid sideways under the hand that had just
   * aimed at something, which is the one case where the viewport must hold
   * still.
   *
   * Deliberately not `$state`: it is read inside `untrack` and must not be a
   * dependency of the effect it gates.
   */
  let _focusFromKey = false

  /**
   * The cell cursor keeps itself visible horizontally.
   *
   * Tab and the arrow keys moved `focusedCol` and left the scroll where it was,
   * so tabbing right walked the cursor off the edge of the viewport and the grid
   * just sat there - the column it claimed to be on was somewhere past the right
   * edge. Rows already did this (`scrollRowIntoView` in the key handlers); this
   * is the other axis, done once here rather than in each of the six handlers
   * that can move the column.
   *
   * `auto`, not `smooth`: a held arrow key fires faster than a smooth scroll
   * settles, so each press would restart an animation that never arrives.
   * Reads are untracked - the geometry it looks at (`geom`, `_scrollLeft`) is
   * exactly what this write changes.
   */
  $effect(() => {
    const ci = focusedCol
    if (ci === null) return
    untrack(() => {
      if (!_focusFromKey) return
      const col = navigableColumns[ci]
      if (col) scrollColumnIntoView(col.name, "auto")
    })
  })

  /**
   * Scroll a visible column into view (if it's off-screen) and briefly highlight
   * it.
   * @param {string} name
   */
  function focusColumnByName(name) {
    if (!scrollColumnIntoView(name)) return
    focusColName = name
    if (_focusColTimer) clearTimeout(_focusColTimer)
    _focusColTimer = setTimeout(() => {
      focusColName = null
      _focusColTimer = null
      scheduleDraw()
    }, 2200)
    scheduleDraw()
  }

  const fkByColumn = $derived(
    Object.fromEntries(columns.map((c) => [c.name, findForeignKeyForColumn(foreignKeys, c.name)])),
  );

  /** @param {number} rowIdx @param {number} colIdx */
  function foreignKeyForCell(rowIdx, colIdx) {
    const col = columns[colIdx];
    if (!col) return null;
    return fkByColumn[col.name] ?? null;
  }

  /**
   * @param {number} rowIdx
   * @param {number} colIdx
   * @param {MouseEvent} [e]
   * @param {{ requireModifier?: boolean, newTab?: boolean }} [opts]
   */
  function tryFollowForeignKey(rowIdx, colIdx, e, opts = {}) {
    if (!foreignKeyForCell(rowIdx, colIdx)) return false;
    const cellValue = rows[rowIdx]?.[colIdx];
    if (cellValue === null || cellValue === undefined) return false;
    if (opts.requireModifier && e && !(e.metaKey || e.ctrlKey || e.altKey))
      return false;
    if (e) {
      e.preventDefault();
      e.stopPropagation();
    }
    // `newTab` forces a second tab for a table that is already open, instead of
    // activating the one that exists - the difference between "take me there"
    // and "put it beside what I am looking at".
    onfollowforeignkey({ rowIdx, colIdx, newTab: opts.newTab === true });
    return true;
  }

  function openInInspector(rowIdx) {
    focusRow(rowIdx);
    inspectorRow = rowIdx;
  }

  /** @param {() => void} action */
  function runMenuAction(action) {
    if (suppressMenuSelect) return;
    action();
  }

  function armMenuSelectGuard() {
    suppressMenuSelect = true;
    const release = () => {
      window.removeEventListener("pointerdown", release);
      window.removeEventListener("pointercancel", release);
      suppressMenuSelect = false;
    };
    // pointerdown fires before click/pointerup, so the guard is always cleared
    // before onSelect fires. On Linux, contextmenu fires on right-click pointerup,
    // meaning armMenuSelectGuard runs after that pointerup has already passed -
    // listening for pointerup would catch the menu item's own pointerup and the
    // setTimeout(0) would still be pending when onSelect fired, blocking all items.
    window.addEventListener("pointerdown", release);
    window.addEventListener("pointercancel", release);
  }

  /**
   * @param {number} rowIdx
   * @param {number} colIdx
   * @param {string} [initialChar] When set, the cell starts with this character
   *   instead of the existing value (type-to-edit behavior).
   */
  function startEdit(rowIdx, colIdx, initialChar) {
    const col = columns[colIdx];
    if (!col) return;

    // A hidden column has no on-canvas cell, so the edit overlay can't anchor to
    // it - setting editingCell would trap keyboard nav until Esc. Bail out.
    if (hiddenColumns.has(col.name)) return;

    // Vectors and geometries open their own viewers rather than an inline text
    // box: 1,500 characters of `0.1,0.1,…` (or a 500-vertex EWKT polygon) in a
    // one-line input is not an edit surface.
    //
    // They open BEFORE the edit gates, because looking at a value is not editing
    // it. Behind the gates, a read-only session or a table with no primary key
    // got "Cannot edit - this table has no primary key" where the embedding
    // preview used to be, which is the preview disappearing for exactly the
    // tables most likely to hold one. The viewer takes the same read-only flag
    // and drops its Save.
    focusedRow = rowIdx;
    if (openVectorViewer(rowIdx, colIdx)) return;
    if (openGeometryViewer(rowIdx, colIdx)) return;

    if (readonly) return;

    if (!primaryKey.length) {
      toast.error("Cannot edit", {
        // ClickHouse is OLAP: rows aren't primary-key addressable, so the backend
        // reports no PK on purpose. Say why and point to the supported path rather
        // than the misleading generic "no primary key" message.
        description: dialect === "clickhouse"
          ? "ClickHouse tables are browse-only here, modify data with ALTER TABLE … UPDATE in the SQL console."
          : "This table has no primary key.",
      });
      return;
    }

    const dataType = col.dataType ?? col.data_type ?? "";
    if (!isEditableType(dataType)) {
      toast.error("Cannot edit column", {
        description: `${col.name} (${dataType}) is not editable.`,
      });
      return;
    }

    const startValue = effectiveCellValue(rowIdx, colIdx);
    const oversize = oversizeCellInfo(startValue);
    if (oversize) {
      // Only a truncated preview was loaded - editing would write it back.
      toast.error("Value too large to edit", {
        description: `${col.name} holds ${formatByteSize(oversize.bytes)}; edit it with a SQL UPDATE instead.`,
      });
      return;
    }
    lastEditOriginalValue = startValue;
    selectOnEditFocus = initialChar === undefined;
    const original = valueToEditString(startValue);
    editingCell = {
      rowIdx,
      colIdx,
      draft: initialChar !== undefined ? initialChar : original,
      original,
    };
    // Enum columns edit via a dropdown - open it immediately so a single
    // interaction (double-click / Enter) reveals the choices.
    enumEditorOpen = !!getColumnEnumValues(col);
  }

  function cancelEdit() {
    if (!editingCell) return;
    editingCell = null;
    enumEditorOpen = false;
    tick().then(() => tableContainer?.focus({ preventScroll: true }));
  }

  /** Stable map key for a staged edit. */
  const editKey = (/** @type {number} */ rowIdx, /** @type {number} */ colIdx) => `${rowIdx}:${colIdx}`;

  /** The value a cell currently shows: the staged edit if any, else the DB value. */
  /** An expanded row's JSON, staged edits included. `_version` is only there so
   *  the template re-reads it when a row is swapped in place. */
  function expandedRecord(/** @type {number} */ rowIdx, /** @type {number} */ _version) {
    return rowToRecord(columns, columns.map((_, i) => effectiveCellValue(rowIdx, i)), hiddenColumns);
  }

  function effectiveCellValue(/** @type {number} */ rowIdx, /** @type {number} */ colIdx) {
    const staged = pendingEdits.get(editKey(rowIdx, colIdx));
    return staged ? staged.value : rows[rowIdx]?.[colIdx];
  }

  /**
   * Stage (or unstage) a cell edit locally - does not touch the DB.
   * If the value matches the row's persisted value, the staged edit is dropped.
   * @param {number} rowIdx @param {number} colIdx @param {unknown} value
   */
  function stageEdit(rowIdx, colIdx, value) {
    const next = new Map(pendingEdits);
    const key = editKey(rowIdx, colIdx);
    const dbValue = rows[rowIdx]?.[colIdx];
    if (valuesEqual(value, dbValue)) {
      next.delete(key);
    } else {
      next.set(key, { rowIdx, colIdx, value, original: dbValue });
    }
    pendingEdits = next;
  }

  /** Loose equality for cell values (handles object/array via JSON). */
  function valuesEqual(/** @type {unknown} */ a, /** @type {unknown} */ b) {
    if (a === b) return true;
    if (a === null || b === null || a === undefined || b === undefined) return false;
    if (typeof a === "object" || typeof b === "object") {
      try { const sa = JSON.stringify(a); return sa === JSON.stringify(b); } catch { return false; }
    }
    return false;
  }

  /** @param {'down'|'right'|'left'|null} afterAction @param {boolean} [autoEdit] */
  async function commitEditWithAction(afterAction, autoEdit = false) {
    if (!editingCell || saving) return;

    const { rowIdx, colIdx, draft } = editingCell;
    const col = columns[colIdx];
    if (!col) return;

    if (draft === editingCell.original) {
      editingCell = null;
      if (afterAction) navigateAfterEdit(rowIdx, colIdx, afterAction, autoEdit);
      else tick().then(() => tableContainer?.focus({ preventScroll: true }));
      return;
    }

    const parsed = parseCellInput(
      draft,
      col.dataType ?? col.data_type ?? "text",
      getColumnEnumValues(col),
    );
    if (!parsed.ok) {
      toast.error("Invalid value", { description: parsed.message });
      return;
    }

    // Stage the change instead of writing immediately - the user applies all
    // pending edits at once from the StatusBar (or discards them with Reset).
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    stageEdit(rowIdx, colIdx, parsed.value);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: parsed.value }];
    futureEdits = [];
    editingCell = null;
    if (afterAction) navigateAfterEdit(rowIdx, colIdx, afterAction, autoEdit);
    else tick().then(() => tableContainer?.focus({ preventScroll: true }));
  }

  async function commitEdit() {
    return commitEditWithAction(null);
  }

  /**
   * Commit the current edit straight to the database, skipping the staged
   * Apply/Reset queue (Ctrl/Cmd+Shift+Enter).
   * @param {'down' | 'right' | 'left' | null} afterAction
   */
  async function commitEditImmediate(afterAction) {
    if (!editingCell || saving) return;
    const { rowIdx, colIdx, draft } = editingCell;
    const col = columns[colIdx];
    if (!col) return;

    if (draft === editingCell.original) {
      editingCell = null;
      if (afterAction) navigateAfterEdit(rowIdx, colIdx, afterAction);
      else tick().then(() => tableContainer?.focus({ preventScroll: true }));
      return;
    }

    const parsed = parseCellInput(
      draft,
      col.dataType ?? col.data_type ?? "text",
      getColumnEnumValues(col),
    );
    if (!parsed.ok) {
      toast.error("Invalid value", { description: parsed.message });
      return;
    }

    try {
      await onsave({ rowIdx, colIdx, value: parsed.value });
      // Drop any staged edit for this cell - it's now persisted.
      const key = editKey(rowIdx, colIdx);
      if (pendingEdits.has(key)) {
        const next = new Map(pendingEdits);
        next.delete(key);
        pendingEdits = next;
      }
      // Drop undo/redo history for this cell too - the value is already in the DB,
      // so an undo must not re-stage the old value and a later Apply re-write it.
      pastEdits = pastEdits.filter((e) => !(e.rowIdx === rowIdx && e.colIdx === colIdx));
      futureEdits = futureEdits.filter((e) => !(e.rowIdx === rowIdx && e.colIdx === colIdx));
      editingCell = null;
      toast.success("Saved", { description: `${col.name} updated` });
      if (afterAction) navigateAfterEdit(rowIdx, colIdx, afterAction);
      else tick().then(() => tableContainer?.focus({ preventScroll: true }));
    } catch (err) {
      toast.error("Could not save", { description: String(err) });
    }
  }

  /**
   * Validated values for the inline new-row draft, if the user has typed anything
   * into it and it passes validation; otherwise null. Lets the combined "Review
   * changes" flow include the pending INSERT instead of silently dropping it.
   * @returns {Record<string, unknown> | null}
   */
  function pendingInsertValues() {
    const staged = newRowDrafts?.[0];
    if (!staged) return null;
    const hasAny = Object.values(staged).some((v) => v !== "" && v != null);
    if (!hasAny) return null;
    const editableCols = columns.filter((c) => isEditableType(c.dataType ?? c.data_type ?? ""));
    const built = buildInsertPayload(editableCols, primaryKey, staged);
    return built.ok ? /** @type {Record<string, unknown>} */ (built.values) : null;
  }

  /**
   * The SQL for everything currently staged, in execution order.
   *
   * Pulled out of `applyPendingEdits` so "Copy to SQL" and "Apply" can never
   * describe two different writes: the statements you copy are the statements
   * that would run.
   */
  function pendingChangeSql() {
    const insertValues = pendingInsertValues();
    // A row staged for deletion doesn't need its cell updates written first.
    const editEntries = [...pendingEdits.values()].filter((e) => !pendingDeletes.has(e.rowIdx));
    const deleteIndices = [...pendingDeletes].sort((a, b) => a - b);
    // Order: updates, deletes, then the insert. Inserting last keeps the existing
    // row indices (which the updates/deletes address) stable during execution.
    const statements = [
      ...buildUpdateStatements(editEntries, rows, dmlContext),
      ...(deleteIndices.length ? buildDeleteStatements(deleteIndices, rows, dmlContext) : []),
      ...(insertValues ? buildInsertStatements(insertValues, dmlContext) : []),
    ];
    return { insertValues, editEntries, deleteIndices, statements };
  }

  /**
   * Put the staged changes on the clipboard as SQL and leave them staged.
   *
   * The way to get this SQL before was to open the preview dialog, select the
   * text and cancel - and cancelling is one keystroke away from applying. A
   * migration you want to paste into a review is not a write you want to run.
   */
  async function copyPendingChangeSql() {
    const { statements, editEntries, deleteIndices, insertValues } = pendingChangeSql();
    if (!statements.length) return;
    const raw = statements.join("\n");
    const fmt = formatSqlFn ?? (await loadFormatter().catch(() => null));
    const ok = await writeClipboard(fmt ? fmt(raw) : raw);
    if (!ok) { toast.error("Could not copy to clipboard"); return; }
    const parts = [];
    if (editEntries.length) parts.push(`${editEntries.length} update${editEntries.length === 1 ? "" : "s"}`);
    if (deleteIndices.length) parts.push(`${deleteIndices.length} delete${deleteIndices.length === 1 ? "" : "s"}`);
    if (insertValues) parts.push("1 insert");
    toast.success(`Copied ${statements.length} statement${statements.length === 1 ? "" : "s"}`, {
      description: `${parts.join(", ")}. Your changes are still staged.`,
    });
  }

  /** Open the DML review for all staged changes (edits + deletes + a pending insert). */
  function applyPendingEdits() {
    if ((!hasPendingChanges && !pendingInsertValues()) || saving) return;
    const review = pendingChangesReview();
    if (review) requestWrite(review);
  }

  /**
   * The review for everything staged, or null when nothing is.
   * @returns {DmlReview | null}
   */
  function pendingChangesReview() {
    const { insertValues, editEntries, deleteIndices, statements } = pendingChangeSql();
    if (!statements.length) return null;
    const parts = [];
    if (editEntries.length) parts.push(`${editEntries.length} cell${editEntries.length === 1 ? "" : "s"} updated`);
    if (deleteIndices.length) parts.push(`${deleteIndices.length} row${deleteIndices.length === 1 ? "" : "s"} deleted`);
    if (insertValues) parts.push("1 row inserted");
    return {
      kind: deleteIndices.length ? "delete" : insertValues && !editEntries.length ? "insert" : "update",
      title: `Review ${statements.length} change${statements.length === 1 ? "" : "s"}`,
      description: `${parts.join(", ")}.${deleteIndices.length ? " Deletions cannot be undone." : ""}`,
      statements,
      confirmLabel: "Apply changes",
      destructive: deleteIndices.length > 0,
      run: executePendingChanges,
      rebuild: pendingChangesReview,
    };
  }

  /** Flush all staged edits and deletes (and a pending insert) to the database. */
  async function executePendingChanges() {
    const insertValues = pendingInsertValues();
    if ((!hasPendingChanges && !insertValues) || saving) return;
    // Edits first (skipping rows about to be deleted), then the deletes.
    const entries = [...pendingEdits.values()].filter((e) => !pendingDeletes.has(e.rowIdx));
    /** @type {typeof entries} */
    const failed = [];
    let okCount = 0;
    for (const edit of entries) {
      try {
        await onsave({ rowIdx: edit.rowIdx, colIdx: edit.colIdx, value: edit.value });
        okCount++;
      } catch (err) {
        failed.push(edit);
        toast.error("Could not save", { description: String(err) });
      }
    }

    let deletedCount = 0;
    const deleteIndices = [...pendingDeletes].sort((a, b) => a - b);
    if (deleteIndices.length) {
      try {
        await ondelete({ rowIndices: deleteIndices });
        deletedCount = deleteIndices.length;
        pendingDeletes = new Set();
      } catch (err) {
        toast.error("Could not delete", { description: String(err) });
      }
    }

    // Insert last so the row indices addressed by the updates/deletes above stay
    // valid throughout. oninsertrow surfaces its own toast + refetch.
    if (insertValues) {
      try {
        await oninsertrow(insertValues);
        cancelNewRow();
      } catch {
        // error toast already shown by oninsertrow
      }
    }

    // Keep only the edits that failed so the user can retry / reset them - unless
    // deletes ran, which splice `rows` and invalidate the failed edits' row indices;
    // in that case drop them so a retry can't target the wrong row.
    const next = new Map();
    if (deletedCount === 0) {
      for (const edit of failed) next.set(editKey(edit.rowIdx, edit.colIdx), edit);
    }
    pendingEdits = next;
    pastEdits = [];
    futureEdits = [];
    // Sync the cross-unmount cache to the post-apply state (clears it when empty).
    savePendingChanges(_persistKey, pendingEdits, pendingDeletes);
    if (okCount > 0 || deletedCount > 0) {
      const parts = [];
      if (okCount > 0) parts.push(`${okCount} cell${okCount === 1 ? "" : "s"} updated`);
      if (deletedCount > 0) parts.push(`${deletedCount} row${deletedCount === 1 ? "" : "s"} deleted`);
      toast.success("Changes applied", { description: parts.join(", ") });
    }
  }

  /** Discard all staged edits and deletes. */
  function resetPendingEdits() {
    if (!hasPendingChanges) return;
    pendingEdits = new Map();
    pendingDeletes = new Set();
    pastEdits = [];
    futureEdits = [];
    clearPendingChanges(_persistKey);
  }

  /** Collapse any range back to the single focused cell. */
  function clearCellRange() {
    if (selAnchor !== null) selAnchor = null;
  }

  /** Write text to the clipboard, falling back to execCommand when the async
   *  Clipboard API is unavailable/blocked (some Tauri webview configs). */
  async function writeClipboard(/** @type {string} */ text) {
    try {
      if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); return true; }
    } catch { /* fall through to execCommand */ }
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.top = '-1000px';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand('copy');
      document.body.removeChild(ta);
      return ok;
    } catch { return false; }
  }

  /** Plain-text value of a cell for range copy (TSV). */
  function cellCopyText(/** @type {number} */ rowIdx, /** @type {number} */ actualIdx) {
    const v = effectiveCellValue(rowIdx, actualIdx);
    if (v === null || v === undefined) return '';
    if (typeof v === 'object') { try { return JSON.stringify(v); } catch { return String(v); } }
    return String(v);
  }

  /** Copy the current rectangular range to the clipboard as TSV (paste-ready for Sheets/Excel). */
  async function copyCellRange() {
    const range = computeCellRange();
    if (!range) return false;
    const cols = navigableColumns.slice(range.c0, range.c1 + 1);
    const actualIdxs = cols.map((col) => _nameToActualIdx.get(col.name) ?? -1);
    /** @type {string[]} */
    const lines = [];
    for (let r = range.r0; r <= range.r1; r++) {
      lines.push(actualIdxs.map((ai) => cellCopyText(r, ai).replace(/\t/g, ' ').replace(/\r?\n/g, ' ')).join('\t'));
    }
    const nCells = (range.r1 - range.r0 + 1) * (range.c1 - range.c0 + 1);
    if (await writeClipboard(lines.join('\n'))) {
      toast.success(`Copied ${nCells} cell${nCells === 1 ? '' : 's'}`);
      return true;
    }
    toast.error('Could not copy');
    return false;
  }

  // Surface staged-edit state to the parent (→ StatusBar Apply/Reset buttons).
  $effect(() => {
    // Apply means "commit what I have staged", and a filled-in draft row is part
    // of that - so it takes the press, and the edits take the next one.
    //
    // Not one atomic action on purpose: the insert goes through the DML confirm,
    // which is a dialog the user can cancel, so awaiting it here would leave
    // Apply hanging on a promise that never settles. One press, one commit, and
    // the count says how much is left.
    applyEdits = async () => {
      if (newRowDrafts) { submitNewRow(); return; }
      await applyPendingEdits();
    };
    copyEditsSql = copyPendingChangeSql;
    resetEdits = () => {
      if (newRowDrafts) cancelNewRow();
      resetPendingEdits();
    };
  });
  // An open insert draft counts. It is a change you have made and not applied,
  // which is exactly what that number means everywhere else - and without it the
  // Apply/Reset pair stayed hidden while a filled-in row sat under the header
  // with no way to commit it but a click on a 12px tick.
  $effect(() => {
    pendingEditCount = pendingEdits.size + pendingDeletes.size + draftCount;
  });

  // Surface scroll-to-top / scroll-to-bottom to the parent (→ StatusBar buttons).
  $effect(() => {
    scrollToTop = () => tableContainer?.scrollTo({ top: 0 });
    scrollToBottom = () => { if (tableContainer) tableContainer.scrollTo({ top: tableContainer.scrollHeight }); };
    scrollToLeft = () => tableContainer?.scrollTo({ left: 0 });
    scrollToRight = () => { if (tableContainer) tableContainer.scrollTo({ left: tableContainer.scrollWidth }); };
    focusColumn = focusColumnByName;
    // Save/restore the PHYSICAL scroll position so it round-trips regardless of
    // whether the scroll range is compressed (same table → same scale).
    getScroll = () => ({ left: _scrollLeft, top: _physScrollTop });
    getExpanded = () => [...expandedRows];
    applyScroll = (pos) => {
      // Wait for the new tab's columns/rows to lay out (spacer width) before
      // setting scroll - otherwise the container clamps to 0.
      tick().then(() => requestAnimationFrame(() => {
        const el = tableContainer;
        if (!el) return;
        el.scrollLeft = Math.max(0, pos.left ?? 0);
        el.scrollTop = Math.max(0, pos.top ?? 0);
        _scrollLeft = el.scrollLeft;
        _physScrollTop = Math.round(el.scrollTop);
        _scrollTop = physToVirt(_physScrollTop);
        scheduleDraw();
      }));
    };
    // Restoring a back/forward position. Clamped to the data that's actually
    // loaded, so a history entry that outlived a filter/delete lands on the
    // nearest real cell instead of focusing past the end.
    focusCell = (row, col) => {
      // The parent holds this closure past unmount (switching to the structure
      // view tears the grid down), so bail while there's no live container
      // rather than touch geometry that belongs to a destroyed component.
      if (!tableContainer) return;
      const rowLen = rows.length;
      const visLen = navigableColumns.length;
      if (rowLen === 0 || visLen === 0) return;
      focusedRow = Math.min(Math.max(row, 0), rowLen - 1);
      focusedCol = Math.min(Math.max(col ?? 0, 0), visLen - 1);
      selAnchor = null; // a restored position is one cell, never a range
      scrollRowIntoView(focusedRow);
      scrollColumnIntoView(navigableColumns[focusedCol].name);
      scheduleDraw();
    };
  });

  // Snapshot/background panes: restore the saved scroll once on mount. The live
  // grid passes initialScroll=null and restores via applyScroll on tab switch.
  let _didSeedScroll = false;
  $effect(() => {
    if (_didSeedScroll || !initialScroll || !tableContainer) return;
    _didSeedScroll = true;
    applyScroll(initialScroll);
  });

  /**
   * Open the insert draft.
   *
   * The draft is one band pinned under the header - that is where every field
   * is reachable and where it stays put while you scroll. `anchorRow` does not
   * move the band; it scrolls the table so that row sits immediately beneath it,
   * which is what makes "insert above this row" and "insert below this one"
   * land where the menu said they would. A table's rows have no stored order,
   * so the position is about where you are looking, not about the data.
   * @param {number | null} [anchorRow]
   */
  /**
   * The staged cell a right-click opened the menu on, and where to draw it.
   * @type {{ row: number, col: string, x: number, y: number } | null}
   */
  let draftMenu = $state(null)

  /** The cell the menu was last closed over, so focus can return to it. */
  let closedFromDraft = /** @type {{ row: number, col: string } | null} */ (null)

  /** @param {MouseEvent} e @param {number} row @param {string} col */
  function openDraftMenu(e, row, col) {
    e.preventDefault()
    e.stopPropagation()
    draftMenu = { row, col, x: e.clientX, y: e.clientY }
  }

  /** What the menu can put in a staged cell, given the column's type. */
  const draftMenuActions = $derived.by(() => {
    /** @typedef {{ id: string, label: string, icon: any, run: () => void }} DraftAction */
    if (!draftMenu) return /** @type {DraftAction[]} */ ([])
    const { row, col: colName } = draftMenu
    const col = columns.find((c) => c.name === colName)
    const dt = String(col?.dataType ?? col?.data_type ?? '').toLowerCase()
    const set = (/** @type {string} */ v) => setNewRowDraft(row, colName, v)
    /** @type {DraftAction[]} */
    const out = []
    if (dt.includes('uuid') || dt.includes('char') || dt.includes('text')) {
      out.push({ id: 'uuid', label: 'Generate UUID', icon: Dices, run: () => set(generateUuid()) })
      out.push({ id: 'cuid', label: 'Generate CUID', icon: Dices, run: () => set(generateCuid()) })
    }
    if (isDateTimeType(dt) || shouldUseDateTimePicker(dt, colName)) {
      out.push({ id: 'now', label: 'Now', icon: Clock, run: () => set(nowDateTimeLocal()) })
    } else if (isDateOnlyType(dt)) {
      out.push({ id: 'today', label: 'Today', icon: CalendarDays, run: () => set(nowDateOnly()) })
    } else if (isTimeOnlyType(dt)) {
      out.push({ id: 'time-now', label: 'Now', icon: Clock, run: () => set(nowTimeOnly()) })
    }
    if (dt.includes('int') || dt.includes('numeric') || dt.includes('decimal') || dt.includes('real') || dt.includes('double')) {
      out.push({ id: 'zero', label: 'Zero', icon: Hash, run: () => set('0') })
    }
    return out
  })

  /** A row of drafts seeded the way opening the band seeds one. */
  function blankDraft() {
    /** @type {Record<string, string>} */
    const drafts = {}
    for (const col of columns) {
      drafts[col.name] = defaultInsertDraft(col, primaryKey)
    }
    return drafts
  }

  function openInsertDraft(anchorRow = null) {
    if (readonly) return
    // Add on an open band appends: pressing it three times is three rows, not
    // the same row reset twice.
    newRowDrafts = [...(newRowDrafts ?? []), blankDraft()]
    newRowFocusIdx = newRowDrafts.length - 1
    // Focus first non-auto column (all columns, including hidden ones)
    const first = columns.find((c) => !isAutoColumn(c, primaryKey))
    newRowFocusCol = first?.name ?? columns[0]?.name ?? null
    if (anchorRow === null) {
      tableContainer?.scrollTo({ top: 0, behavior: 'smooth' })
      return
    }
    // After the draft mounts: `insertRowOffset` is part of rowDocTop, so the
    // target is only right once the band exists.
    const idx = Math.min(Math.max(anchorRow, 0), Math.max(0, rows.length - 1))
    void tick().then(() => {
      if (!tableContainer) return
      const top = Math.max(0, rowDocTop(idx) - HEADER_H - ROW_HEIGHT)
      tableContainer.scrollTo({ top, behavior: 'smooth' })
    })
  }

  // Surface beginInsertRow to the parent (→ toolbar Add Row button).
  $effect(() => {
    beginInsertRow = () => openInsertDraft()
  })

  // Surface staged-delete-of-selection to the parent (⌘⌫ / toolbar delete).
  $effect(() => {
    stageDeleteSelected = () => {
      if (readonly) return;
      // Checked rows if there are any, otherwise the row under the cell cursor.
      // The chord is printed on a context menu item that reads "Delete row" and
      // acts on the row you opened it over, so requiring a checkbox first made
      // the shortcut do nothing on exactly the row it was offered for.
      const targets = selected.size > 0 ? [...selected] : focusedRow !== null ? [focusedRow] : [];
      if (targets.length === 0) return;
      if (!primaryKey.length) {
        toast.error("Cannot delete", { description: "This table has no primary key." });
        return;
      }
      const next = new Set(pendingDeletes);
      for (const ri of targets) next.add(ri);
      pendingDeletes = next;
      scheduleDraw();
    }
  })

  function cancelNewRow() {
    newRowDrafts = null
    newRowFocusCol = null
    newRowFocusIdx = 0
  }

  /**
   * Put one staged cell's value into the same column of every staged row.
   * With several rows in the band this is the difference between typing a
   * tenant id five times and typing it once.
   * @param {number} row @param {string} col
   */
  function fillDraftColumn(row, col) {
    if (!newRowDrafts?.length) return
    const v = newRowDrafts[row]?.[col] ?? ''
    newRowDrafts = newRowDrafts.map((d) => ({ ...d, [col]: v }))
  }

  /** Drop one staged row. The last one out closes the band. @param {number} i */
  function removeDraftRow(i) {
    if (!newRowDrafts) return
    const next = newRowDrafts.filter((_, j) => j !== i)
    newRowDrafts = next.length ? next : null
    newRowFocusIdx = Math.max(0, Math.min(newRowFocusIdx, (newRowDrafts?.length ?? 1) - 1))
  }

  /**
   * Insert the staged rows - one of them, or all of them.
   *
   * All of them is what the header tick and Apply mean. One confirm covers the
   * batch: reviewing three inserts is one list of three statements, not three
   * dialogs.
   * @param {number | null} [only] index of a single staged row, or null for all
   */
  function submitNewRow(only = null) {
    if (!newRowDrafts?.length || insertSaving) return
    const editableCols = columns.filter(c => isEditableType(c.dataType ?? c.data_type ?? ''))
    /** @type {{ i: number, values: Record<string, unknown> }[]} */
    const batch = []
    const indices = only === null ? newRowDrafts.map((_, i) => i) : [only]
    for (const i of indices) {
      const drafts = newRowDrafts[i]
      if (!drafts) continue
      // A required column left blank is a failure the row already knows about.
      // Sending it to find out costs a round trip and comes back as a message
      // about a field nobody is looking at.
      const missing = insertMissing[i] ?? []
      if (missing.length) {
        focusFirstMissing(i)
        toast.error(
          missing.length === 1
            ? `${missing[0]} is required`
            : `${missing.length} required fields are empty`,
          { description: `Row ${i + 1}: ${missing.join(', ')}` },
        )
        return
      }
      const built = buildInsertPayload(editableCols, primaryKey, drafts)
      if (!built.ok) {
        toast.error(`Cannot insert row ${i + 1}`, { description: built.message })
        return
      }
      batch.push({ i, values: /** @type {Record<string, unknown>} */ (built.values) })
    }
    if (!batch.length) return
    requestWrite({
      kind: "insert",
      title: batch.length === 1 ? "Review insert" : `Review ${batch.length} inserts`,
      description: batch.length === 1 ? "A new row will be inserted." : `${batch.length} new rows will be inserted.`,
      statements: batch.flatMap((b) => buildInsertStatements(b.values, dmlContext)),
      confirmLabel: batch.length === 1 ? "Insert row" : `Insert ${batch.length} rows`,
      destructive: false,
      run: () => executeInsertRows(batch),
    })
  }

  /**
   * Run the batch in order, and keep whatever did not land.
   *
   * A row that fails stays staged with everything still typed into it - losing
   * four filled-in rows because the third one violated a constraint is not a
   * trade worth making.
   * @param {{ i: number, values: Record<string, unknown> }[]} batch
   */
  async function executeInsertRows(batch) {
    /** @type {Set<number>} */
    const done = new Set()
    let failed = false
    for (const { i, values } of batch) {
      try {
        await oninsertrow(values)
        done.add(i)
      } catch {
        // oninsertrow has already said what went wrong.
        failed = true
        break
      }
    }
    if (done.size) {
      const left = (newRowDrafts ?? []).filter((_, i) => !done.has(i))
      newRowDrafts = left.length ? left : null
      if (!newRowDrafts) { newRowFocusCol = null; newRowFocusIdx = 0 }
      else newRowFocusIdx = Math.min(newRowFocusIdx, newRowDrafts.length - 1)
    }
    if (failed && done.size) {
      toast.info(`Inserted ${done.size} of ${batch.length}`, {
        description: 'The rows that did not go in are still staged.',
      })
    }
  }

  /**
   * The required columns still blank. The draft used to say "Required" in a
   * placeholder and then let you press Insert anyway, so the first thing it told
   * you about a missing value was an error from the database.
   */
  const insertMissing = $derived.by(() => {
    if (!newRowDrafts?.length) return /** @type {string[][]} */ ([])
    const required = columns.filter((c) => insertOmitBehaviour(c, primaryKey) === 'required')
    return newRowDrafts.map((d) =>
      required.filter((c) => !String(d?.[c.name] ?? '').trim()).map((c) => c.name),
    )
  })

  /**
   * Put the caret on the first column a staged row still needs.
   *
   * Said before the database says it: a failed insert names the column in a
   * toast, which is the right words in the wrong place - the field it is about
   * is on screen and nothing points at it.
   * @param {number} rowIdx
   */
  function focusFirstMissing(rowIdx) {
    const miss = insertMissing[rowIdx]?.[0]
    if (!miss) return false
    newRowFocusIdx = rowIdx
    newRowFocusCol = miss
    return true
  }

  /**
   * Whether a draft holds anything a person put there.
   *
   * A fresh draft is not empty - date columns seed themselves with now, and a
   * generated column carries its placeholder - so "has content" means it differs
   * from what opening it would produce.
   * @param {Record<string, string>[] | null} drafts
   */
  function draftHasContent(drafts) {
    return (drafts ?? []).some((row) =>
      columns.some((col) => {
        const seeded = defaultInsertDraft(col, primaryKey)
        return String(row?.[col.name] ?? '') !== String(seeded ?? '')
      }),
    )
  }

  /** @param {number} rowIdx @param {string} colName @param {string} value */
  function setNewRowDraft(rowIdx, colName, value) {
    if (!newRowDrafts?.[rowIdx]) return
    newRowDrafts = newRowDrafts.map((d, i) => (i === rowIdx ? { ...d, [colName]: value } : d))
  }

  /** @param {KeyboardEvent} e */
  /** @param {KeyboardEvent} e @param {number} [rowIdx] the staged row the field belongs to */
  function onNewRowKeydown(e, rowIdx = 0) {
    // Escape drops the row you are in; the band only closes when it was the
    // last one. Discarding four filled-in rows because you pressed Escape in
    // the fourth is not what that key means.
    if (e.key === 'Escape') {
      e.preventDefault()
      // Mod+Escape discards the whole band; plain Escape drops this row.
      if (e.ctrlKey || e.metaKey) cancelNewRow()
      else removeDraftRow(rowIdx)
      return
    }
    // ⌘↵ inserts everything staged - the batch is the point of stacking them.
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); void submitNewRow(); return }
    // Alt+↵ adds another row below, for filling several in without reaching for
    // the Add button between each one.
    if (e.altKey && e.key === 'Enter') { e.preventDefault(); openInsertDraft(); return }

    // Tab / Enter: move right between cells (not down to the next row).
    // Shift+Tab moves left. Enter at the last cell submits.
    if (e.key === 'Tab' || e.key === 'Enter') {
      e.preventDefault()
      // Every column takes a value now, including generated ones, so Tab must
      // be able to reach them - initial focus still skips them (see beginInsertRow),
      // because overriding a sequence is the exception rather than the flow.
      // Visible ones only: Tab cannot land on a field the band does not draw.
      const editableCols = visibleColumns
      if (!editableCols.length) return
      const curIdx = editableCols.findIndex(c => c.name === newRowFocusCol)
      if (e.shiftKey) {
        const prev = curIdx <= 0 ? editableCols.length - 1 : curIdx - 1
        newRowFocusCol = editableCols[prev].name
      } else {
        const next = curIdx + 1
        if (next >= editableCols.length) {
          if (e.key === 'Enter') void submitNewRow(rowIdx)
          else newRowFocusCol = editableCols[0].name  // Tab wraps to first
        } else {
          newRowFocusCol = editableCols[next].name
        }
      }
    }
  }

  // Auto-focus the new-row input when focus column changes.
  $effect(() => {
    // Only what asks for focus is a dependency. Reading the drafts here made
    // every keystroke re-run this, and a re-run moves the caret: typing in the
    // second staged row put the next character in the first one.
    const col = newRowFocusCol
    const bandIdx = newRowFocusIdx
    if (!col) return
    untrack(() => {
      if (!newRowDrafts?.length) return
      tick().then(() => {
        // Scoped to the staged row that asked for focus. A global lookup by
        // column name lands in the first band every time, so with three rows
        // staged the caret jumped back to the top one on every move.
        const band = document.querySelector(`[data-new-row="${bandIdx}"]`)
        const el = /** @type {HTMLElement|null} */ (
          band?.querySelector(`[data-new-row-input="${col}"]`) ?? null
        )
        // Already there: focusing again would put the caret back at the end of
        // whatever was just typed.
        if (el && document.activeElement !== el) {
          // The band is pinned to the viewport, so the browser has nothing to
          // scroll when focus lands on a field whose column is off to the right
          // - it would leave the caret on a cell nobody can see. Suppress its
          // attempt and move the grid to the column instead, the same way the
          // cell cursor does when Tab walks it past the edge.
          el.focus({ preventScroll: true })
          scrollColumnIntoView(col, 'auto')
        }
      })
    })
  })


  async function copyCellValue(rowIdx, colIdx) {
    const value = rows[rowIdx]?.[colIdx];
    try {
      await navigator.clipboard.writeText(formatNormalValue(value));
      toast.success("Copied");
    } catch {
      toast.error("Could not copy to clipboard");
    }
  }

  /**
   * Copy a cell as a hex byte string (Postgres bytea style: `\x` + UTF-8 bytes).
   * Numbers/booleans use their string representation; objects use JSON.
   * @param {number} rowIdx @param {number} colIdx
   */
  async function copyCellHex(rowIdx, colIdx) {
    const value = effectiveCellValue(rowIdx, colIdx);
    let hex = "";
    if (value !== null && value !== undefined) {
      const s = typeof value === "object" ? cellJsonString(value) : String(value);
      for (const b of new TextEncoder().encode(s)) hex += b.toString(16).padStart(2, "0");
    }
    if (await writeClipboard("\\x" + hex)) toast.success("Copied as hex");
    else toast.error("Could not copy to clipboard");
  }

  async function copyRowJson(rowIdx) {
    const record = rowToRecord(columns, rows[rowIdx] ?? [], hiddenColumns);
    try {
      await navigator.clipboard.writeText(formatJsonValue(record));
      toast.success("Copied row as JSON");
    } catch {
      toast.error("Could not copy to clipboard");
    }
  }

  // ── Copy row as … ──────────────────────────────────────────────────────────

  /** Indices to copy: all selected rows if contextRow is in selection, else just contextRow. */
  function copyTargetIndices(rowIdx) {
    return selected.size > 1 && selected.has(rowIdx)
      ? [...selected].sort((a, b) => a - b)
      : [rowIdx];
  }

  /** Full text of an object cell for copy/export - oversize sentinels become
   * their marker + preview so exports show the truncation explicitly. */
  function cellJsonString(value) {
    const over = oversizeCellInfo(value);
    return over ? oversizeCellText(over) : JSON.stringify(value);
  }

  /** Escape a cell value for CSV (RFC 4180). */
  function csvCell(value) {
    if (value === null || value === undefined) return '';
    const s = typeof value === 'object' ? cellJsonString(value) : String(value);
    if (s.includes(',') || s.includes('"') || s.includes('\n') || s.includes('\r')) {
      return '"' + s.replace(/"/g, '""') + '"';
    }
    return s;
  }

  /** Escape a cell value for SQL INSERT. */
  function sqlLiteral(value) {
    if (value === null || value === undefined) return 'NULL';
    if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
    if (typeof value === 'number') return String(value);
    if (typeof value === 'object') {
      const s = cellJsonString(value).replace(/'/g, "''");
      return `'${s}'`;
    }
    return "'" + String(value).replace(/'/g, "''") + "'";
  }

  /** Markdown-safe cell text. */
  function mdCell(value) {
    if (value === null || value === undefined) return 'NULL';
    const s = typeof value === 'object' ? cellJsonString(value) : String(value);
    return s.replace(/\|/g, '\\|').replace(/\n/g, ' ');
  }

  /**
   * Copy one column's values, one per line, for the rows currently loaded.
   *
   * Scoped to the loaded page on purpose: the grid holds what it fetched, and
   * silently issuing a second full-table read behind a menu item labelled "copy"
   * is not what the label promises. The toast says how many rows went.
   * @param {string} colName
   */
  async function copyColumnValues(colName) {
    const ai = _nameToActualIdx.get(colName) ?? -1
    if (ai < 0) return
    const idx = selected.size > 0 ? [...selected].sort((a, b) => a - b) : rows.map((_, i) => i)
    const text = idx.map((r) => cellCopyText(r, ai)).join('\n')
    if (await writeClipboard(text)) {
      toast.success(`Copied ${idx.length.toLocaleString()} ${idx.length === 1 ? 'value' : 'values'}`, {
        description: colName,
      })
    } else {
      toast.error('Could not copy to clipboard')
    }
  }

  async function copyColSelection() {
    const activeCols = columns.filter((c) => selectedCols.has(c.name))
    if (!activeCols.length) return
    const rowIndices = selected.size > 0
      ? [...selected].sort((a, b) => a - b)
      : rows.map((_, i) => i)
    const header = activeCols.map((c) => csvCell(c.name)).join('\t')
    // Resolve column indices once - indexOf inside the per-row loop is
    // O(rows × selCols × totalCols) on wide tables.
    const colIdxs = activeCols.map((c) => _nameToActualIdx.get(c.name) ?? columns.indexOf(c))
    const body = rowIndices
      .map((i) => colIdxs.map((ci) => {
        const v = rows[i]?.[ci]
        return v === null || v === undefined ? '' : typeof v === 'object' ? cellJsonString(v) : String(v)
      }).join('\t'))
      .join('\n')
    const text = header + '\n' + body
    try {
      await navigator.clipboard.writeText(text)
      const colLabel = activeCols.length === 1 ? activeCols[0].name : `${activeCols.length} columns`
      toast.success(`Copied ${colLabel} (${rowIndices.length} rows)`)
    } catch {
      toast.error('Could not copy to clipboard')
    }
  }

  async function copyAs(rowIdx, format) {
    const indices = copyTargetIndices(rowIdx);
    const allRows = indices.map((i) => rows[i] ?? []);
    let text = '';
    const label = indices.length > 1 ? `${indices.length} rows` : '1 row';

    // Human-readable formats mirror the on-screen columns, so they omit hidden
    // ones; INSERT reconstructs the row and therefore keeps every column.
    const visIdxs = columns.map((_, i) => i).filter((i) => !hiddenColumns.has(columns[i].name));
    const cols = visIdxs.map((i) => columns[i]);
    const colNames = cols.map((c) => c.name);
    const dataRows = visIdxs.length === columns.length ? allRows : allRows.map((r) => visIdxs.map((i) => r[i]));

    if (format === 'csv') {
      const header = colNames.map(csvCell).join(',');
      const body = dataRows.map((r) => r.map(csvCell).join(',')).join('\n');
      text = header + '\n' + body;
    } else if (format === 'json') {
      const records = dataRows.map((r) => rowToRecord(cols, r));
      text = formatJsonValue(indices.length === 1 ? records[0] : records);
    } else if (format === 'plain') {
      text = dataRows
        .map((r) =>
          colNames.map((name, i) => {
            const v = r[i];
            const s = v === null || v === undefined ? 'NULL' : typeof v === 'object' ? cellJsonString(v) : String(v);
            return `${name}: ${s}`;
          }).join('\n'),
        )
        .join('\n\n');
    } else if (format === 'markdown') {
      const sep = colNames.map(() => '---').join(' | ');
      const header = colNames.map(mdCell).join(' | ');
      text = `| ${header} |\n| ${sep} |\n${dataRows.map((r) => `| ${r.map(mdCell).join(' | ')} |`).join('\n')}`;
    } else if (format === 'insert') {
      const tbl = schema ? `"${schema}"."${tableName || 'table'}"` : `"${tableName || 'table'}"`;
      const insertCols = columns.map((c) => `"${c.name}"`).join(', ');
      text = allRows
        .map((r) => `INSERT INTO ${tbl} (${insertCols}) VALUES (${r.map(sqlLiteral).join(', ')});`)
        .join('\n');
    }

    try {
      await navigator.clipboard.writeText(text);
      toast.success(`Copied ${label} as ${format.toUpperCase()}`);
    } catch {
      toast.error('Could not copy to clipboard');
    }
  }

  function setCellNull(rowIdx, colIdx) {
    const col = columns[colIdx];
    if (!col || !canEditColumn(colIdx)) return;
    // A NOT NULL column can't hold NULL - reject up front with a clear message
    // instead of staging an edit that the database will refuse on apply. Matches
    // the inline editor, which hides the NULL option for non-nullable columns.
    if (col.nullable === false) {
      toast.error("Cannot set NULL", { description: `"${col.name}" is NOT NULL.` });
      return;
    }
    if (effectiveCellValue(rowIdx, colIdx) === null) {
      toast.message("Already NULL");
      return;
    }
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    stageEdit(rowIdx, colIdx, null);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: null }];
    futureEdits = [];
  }

  /**
   * Bulk fill: stage `value` into `colIdx` for every currently selected row.
   * Each staged edit flows through the normal Apply DML preview + undo/redo,
   * exactly like an inline edit. Guarded on PK/editable-type like startEdit.
   * @param {number} colIdx @param {unknown} value
   */
  function fillSelectedColumn(colIdx, value) {
    const col = columns[colIdx];
    if (!canEditColumn(colIdx)) {
      toast.error("Cannot edit column", { description: `${col?.name ?? "This column"} is not editable.` });
      return;
    }
    if (value === null && col?.nullable === false) {
      toast.error("Cannot set NULL", { description: `"${col.name}" is NOT NULL.` });
      return;
    }
    const targets = [...selected].sort((a, b) => a - b);
    if (!targets.length) return;
    /** @type {typeof pastEdits} */
    const batch = [];
    for (const rowIdx of targets) {
      // Skip truncated previews - staging one would write the preview back.
      if (oversizeCellInfo(rows[rowIdx]?.[colIdx])) continue;
      const prevValue = effectiveCellValue(rowIdx, colIdx);
      if (valuesEqual(prevValue, value)) continue;
      stageEdit(rowIdx, colIdx, value);
      batch.push({ rowIdx, colIdx, oldValue: prevValue, newValue: value });
    }
    if (!batch.length) {
      toast.message("No changes to stage");
      return;
    }
    pastEdits = [...pastEdits, ...batch].slice(-50);
    futureEdits = [];
    toast.success(`Staged ${col?.name ?? "value"} on ${batch.length} row${batch.length === 1 ? "" : "s"}`, {
      description: "Review in Apply, or undo per row.",
    });
  }

  /** Open the vector viewer for a cell. Returns false when it isn't a vector. */
  function openVectorViewer(rowIdx, colIdx) {
    const col = columns[colIdx];
    if (!col) return false;
    const type = String(col.dataType ?? col.data_type ?? _colCache[colIdx]?.colType ?? "");
    if (!isVectorType(type)) return false;
    const v = effectiveCellValue(rowIdx, colIdx);
    if (typeof v !== "string") return false;
    vectorViewerRow = rowIdx;
    vectorViewerCol = colIdx;
    vectorViewerColName = col.name ?? "vector";
    vectorViewerType = type.replace(/\(.*$/, "").trim() || "vector";
    vectorViewerNullable = col.isNullable ?? col.is_nullable ?? true;
    vectorViewerValue = v;
    vectorViewerReadOnly = !canEditColumn(colIdx);
    vectorViewerOpen = true;
    return true;
  }

  /** @param {string | null} next */
  function commitVectorViewer(next) {
    const rowIdx = vectorViewerRow, colIdx = vectorViewerCol;
    if (!canEditColumn(colIdx)) return;
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    stageEdit(rowIdx, colIdx, next);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: next }];
    futureEdits = [];
  }

  /** Open the geometry viewer for a cell. Returns false when it isn't geometry. */
  function openGeometryViewer(rowIdx, colIdx) {
    const col = columns[colIdx];
    if (!col) return false;
    const type = String(col.dataType ?? col.data_type ?? _colCache[colIdx]?.colType ?? "");
    if (!isGeometryType(type)) return false;
    const v = effectiveCellValue(rowIdx, colIdx);
    if (typeof v !== "string") return false;
    geomViewerRow = rowIdx;
    geomViewerCol = colIdx;
    geomViewerColName = col.name ?? "geometry";
    geomViewerType = type;
    geomViewerNullable = col.isNullable ?? col.is_nullable ?? true;
    geomViewerValue = v;
    geomViewerReadOnly = !canEditColumn(colIdx);
    geomViewerOpen = true;
    return true;
  }

  /** @param {string | null} next */
  function commitGeometryViewer(next) {
    const rowIdx = geomViewerRow, colIdx = geomViewerCol;
    if (!canEditColumn(colIdx)) return;
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    stageEdit(rowIdx, colIdx, next);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: next }];
    futureEdits = [];
  }

  /** Open the dedicated array editor for a cell (from the context menu). */
  /**
   * The column the cell cursor is sitting on. For callers that want to act on
   * the cell you are looking at rather than make you name the column again -
   * the filter bar seeds itself with this.
   */
  export function focusedColumnName() {
    if (focusedCol === null) return "";
    const ai = visToActualColIdx(focusedCol);
    return ai >= 0 ? (columns[ai]?.name ?? "") : "";
  }

  // ── Full-size cell editor (Space) ─────────────────────────────────────────
  /** @type {{ focusEditor: (caret?: 'auto'|'start'|'end') => boolean } | null} */
  let cellEditorRef = $state(null);
  /** Shift+Space opens the dock already focused; plain Space does not. */
  let cellEditorFocusOnOpen = $state(false);
  /** The dock is showing a whole row as JSON (Alt+J), not one cell. */
  let cellEditorRowJson = $state(false);
  let cellEditorOpen = $state(false);
  let cellEditorRow = $state(-1);
  let cellEditorCol = $state(-1);
  let cellEditorName = $state("");
  let cellEditorType = $state("");
  let cellEditorValue = $state(/** @type {unknown} */ (null));
  /**
   * Cells with a fetch in flight, keyed `row:col`. A plain Set, not reactive:
   * the canvas is what renders it, and the draw loop is already running while
   * the spinner turns.
   * @type {Set<string>}
   */
  let _loadingCells = new Set()
  let _spinRaf = 0
  /** Keep repainting while anything is loading, so the spinner actually spins. */
  function tickSpinner() {
    if (_spinRaf) return
    const step = () => {
      _spinRaf = 0
      if (_loadingCells.size === 0) return
      scheduleDraw()
      _spinRaf = requestAnimationFrame(step)
    }
    _spinRaf = requestAnimationFrame(step)
  }

  /**
   * Fetch one capped cell and put the value in the row. The grid keeps its
   * preview for every other row: one cell being read is not a reason to pull the
   * column back into the page.
   * @param {number} rowIdx @param {number} colIdx
   */
  async function loadCellInline(rowIdx, colIdx) {
    if (!onloadcellvalue) return
    const key = `${rowIdx}:${colIdx}`
    if (_loadingCells.has(key)) return
    _loadingCells.add(key)
    scheduleDraw()
    tickSpinner()
    try {
      await onloadcellvalue({ rowIdx, colIdx })
      // The dock is a view of a cell, so a cell that just changed under it has
      // to be re-read. It used to re-read only when the dock already happened to
      // be on this cell, which is not where it usually is: the Load button is in
      // the cell, clicking it does not move the cursor, and the dock follows the
      // cursor - so loading a value left the dock showing some other row and
      // still saying "not loaded". Loading a cell is a request to see that cell.
      if (cellEditorOpen && !cellEditorDetached) {
        focusedRow = rowIdx
        focusedCol = actualToVisColIdx(colIdx) >= 0 ? actualToVisColIdx(colIdx) : focusedCol
        seedCellEditor(rowIdx, colIdx)
      }
    } catch (e) {
      toast.error('Could not load the value', { description: String(e?.message ?? e) })
    } finally {
      _loadingCells.delete(key)
      scheduleDraw()
    }
  }

  /** Set when the dock holds a 16KB preview of a capped cell, not the value. */
  let cellEditorOversize = $state(/** @type {{ bytes: number, dataType: string } | null} */ (null));
  /** The loaded value hit the fetch ceiling: what is in the dock is not all of it. */
  let cellEditorTruncated = $state(false);
  /**
   * True when the dock is showing a value that has no cell behind it - a node
   * picked out of an expanded row, say. It cannot follow the cursor (there is
   * nothing to follow) and it cannot be staged (there is nowhere to write it),
   * so it is read-only and the follow effect leaves it alone.
   */
  let cellEditorDetached = $state(false);

  /**
   * Open the focused cell at full size. A 28px row is the wrong surface for a
   * paragraph, a stack trace or a 40-line payload, and the inline editor shows
   * one line of it.
   * @param {number} rowIdx @param {number} colIdx
   */
  // Closing the dock releases what it was holding. A loaded value can be 8MB of
  // string, and keeping it referenced after the panel is gone is 8MB retained
  // for a panel nobody is looking at - per table, until the next one replaces it.
  $effect(() => {
    if (cellEditorOpen) return
    untrack(() => {
      if (cellEditorValue !== null) cellEditorValue = null
      cellEditorOversize = null
      cellEditorTruncated = false
    })
  })

  /**
   * Open the dock on a cell from outside. For a value too big to put in the
   * row: the dock reads it in pages, so this is the answer rather than a
   * message telling you to press a key yourself.
   */
  export function openCellDock(rowIdx, colIdx) {
    focusedRow = rowIdx
    const vi = actualToVisColIdx(colIdx)
    if (vi >= 0) focusedCol = vi
    openCellEditor(rowIdx, colIdx)
    scrollRowIntoView(rowIdx)
  }

  /**
   * @param {number} rowIdx @param {number} colIdx
   * @param {boolean} [focus] Open with the caret in the editor (Shift+Space).
   *   Set before `cellEditorOpen`, so the panel has it when it first seeds.
   */
  function openCellEditor(rowIdx, colIdx, focus = false) {
    if (!seedCellEditor(rowIdx, colIdx)) return;
    cellEditorFocusOnOpen = focus;
    cellEditorDetached = false;
    // One dock at a time. Both live along the bottom edge, and stacking them
    // leaves the grid a couple of rows tall.
    fkSubview = null;
    cellEditorOpen = true;
  }

  /**
   * Show a value in the dock that did not come from a cell - a node inside an
   * expanded row. Same surface, same tree, read-only.
   * @param {unknown} value @param {string} label
   */
  function openValueInDock(value, label) {
    cellEditorRowJson = false;
    cellEditorOversize = null;
    cellEditorTruncated = false;
    cellEditorRow = -1;
    cellEditorCol = -1;
    cellEditorName = label || 'value';
    cellEditorType = '';
    cellEditorValue = value;
    cellEditorDetached = true;
    fkSubview = null;
    cellEditorOpen = true;
  }

  /**
   * The whole focused row as one JSON object, in the dock (Alt+J).
   *
   * Reading a row across a wide table means scrolling sideways and holding the
   * column names in your head. This is the same row with the names attached,
   * on one screen. Detached on purpose - it is a view of a row, not of a cell,
   * so the cursor moving must not re-point it at whatever cell it lands on.
   * @param {number} rowIdx
   */
  function openRowJson(rowIdx) {
    if (rows[rowIdx] === undefined) return;
    // Through the same helper "Copy row as JSON" uses, so the two agree and
    // hidden columns stay hidden. Built from effectiveCellValue rather than the
    // raw row so staged edits show: this is the row as it stands, which is what
    // the grid above it is showing too.
    const values = columns.map((_, i) => effectiveCellValue(rowIdx, i));
    openValueInDock(rowToRecord(columns, values, hiddenColumns), `row ${rowIdx + 1}`);
    // After openValueInDock, which clears both: the dock is detached so the
    // cell-follow effect leaves it alone, and the row is remembered so the
    // row-follow effect below can move it.
    cellEditorRow = rowIdx;
    cellEditorRowJson = true;
  }

  /**
   * Point the editor at a cell. Split out of `openCellEditor` so the cursor can
   * move the open dock from cell to cell without re-opening it.
   * @param {number} rowIdx @param {number} colIdx
   * @returns {boolean} whether the cell could be read
   */
  function seedCellEditor(rowIdx, colIdx) {
    const col = columns[colIdx];
    if (!col || rowIdx < 0) return false;
    cellEditorRowJson = false;
    const value = effectiveCellValue(rowIdx, colIdx);
    // Only a preview of an oversize cell was ever loaded; editing it would write
    // the preview back over the real value.
    const oversize = oversizeCellInfo(value);
    cellEditorRow = rowIdx;
    cellEditorCol = colIdx;
    cellEditorName = col.name ?? "value";
    cellEditorType = String(col.dataType ?? col.data_type ?? _colCache[colIdx]?.colType ?? "");
    cellEditorValue = oversize ? oversize.preview : value;
    // Kept so the panel can say what it is holding - and so Stage change stays
    // out of reach. Staging the preview would write 16KB over the 287KB that is
    // actually in the row.
    cellEditorOversize = oversize ? { bytes: oversize.bytes, dataType: oversize.dataType } : null;
    cellEditorTruncated = false;
    return true;
  }

  /**
   * The open dock follows the cell cursor.
   *
   * Arrowing through the grid with the editor open used to leave it showing the
   * cell you opened it on, so the panel and the cursor disagreed about which
   * value you were looking at - and the only way to edit the next row was to
   * close the panel and press Space again. Now it reads like an inspector:
   * move the cursor, the panel follows. `untrack` around the write keeps the
   * effect off its own output; the cell coordinates it sets are exactly what it
   * would otherwise re-enter on.
   */
  $effect(() => {
    if (!cellEditorOpen || cellEditorDetached) return;
    const r = focusedRow;
    const cv = focusedCol;
    if (r === null || cv === null) return;
    untrack(() => {
      const ai = visToActualColIdx(cv);
      if (ai < 0) return;
      if (r === cellEditorRow && ai === cellEditorCol) return;
      seedCellEditor(r, ai);
    });
  });

  /**
   * The row-JSON dock follows the row cursor, the way the cell dock follows the
   * cell one. Moving down the grid with it open used to leave it on the row it
   * was opened from, so the highlighted row and the JSON under it disagreed
   * about which row you were reading.
   *
   * Only the row is watched: stepping across columns within a row changes
   * nothing about the row, and re-rendering it there would be work for an
   * identical result. `dataVersion` is watched too, so a staged edit shows up
   * in the JSON the same moment it shows up in the grid.
   */
  $effect(() => {
    if (!cellEditorOpen || !cellEditorRowJson) return;
    const r = focusedRow;
    void dataVersion;
    if (r === null || rows[r] === undefined) return;
    untrack(() => {
      if (r === cellEditorRow && cellEditorValue !== null) {
        // Same row, but the data under it may have changed.
        const values = columns.map((_, i) => effectiveCellValue(r, i));
        cellEditorValue = rowToRecord(columns, values, hiddenColumns);
        return;
      }
      openRowJson(r);
    });
  });

  /**
   * Swap the dock's preview for the whole value. Only the dock gets it - the
   * grid keeps the preview, so one row being read does not put a megabyte back
   * into the page that deliberately left it out.
   */
  async function loadFullCellValue() {
    if (!onfetchcellvalue || cellEditorRow < 0 || cellEditorCol < 0) return
    const res = await onfetchcellvalue({ rowIdx: cellEditorRow, colIdx: cellEditorCol })
    cellEditorValue = res.text
    cellEditorOversize = null
    cellEditorTruncated = res.truncated === true
  }

  /** Stage the edited value - same queue, undo and Apply as an inline edit. */
  function commitCellEditor(/** @type {string} */ next) {
    const rowIdx = cellEditorRow, colIdx = cellEditorCol;
    if (!canEditColumn(colIdx)) return;
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    stageEdit(rowIdx, colIdx, next);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: next }];
    futureEdits = [];
  }

  function openArrayEditor(rowIdx, colIdx) {
    const col = columns[colIdx];
    if (!col) return;
    const v = effectiveCellValue(rowIdx, colIdx);
    arrayEditorRow = rowIdx;
    arrayEditorCol = colIdx;
    arrayEditorColName = col.name ?? "array";
    arrayEditorType = String(col.dataType ?? col.data_type ?? _colCache[colIdx]?.colType ?? "").replace(/\[\]\s*$/, "");
    arrayEditorValue = Array.isArray(v) ? v : [];
    arrayEditorOpen = true;
  }

  /** Save the edited array - stage a Postgres array literal (backend casts it). */
  function commitArrayEditor(next) {
    const rowIdx = arrayEditorRow, colIdx = arrayEditorCol;
    if (!canEditColumn(colIdx)) return;
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    const literal = pgArrayText(next); // {a,b} - quoting/escaping handled
    stageEdit(rowIdx, colIdx, literal);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: literal }];
    futureEdits = [];
  }

  /** Run an extension transform on a cell.
   *
   *  Slugify, UPPERCASE, Trim and the rest produce a replacement for the value,
   *  so they are staged as an edit - identical to typing the result in, and
   *  undoable and savable on the same path. Two cases have nothing to write
   *  back: `informational` transforms, which report on the value rather than
   *  replace it, and any column the grid cannot edit. Those copy the result and
   *  show it in a readable card (monospace, pretty-printed, with a Copy action).
   */
  async function runCellTransform(rowIdx, colIdx, transform) {
    const value = effectiveCellValue(rowIdx, colIdx);
    let out;
    try {
      out = transform.run(value);
    } catch (e) {
      toast.error("Could not apply transform", { description: String(e?.message ?? e) });
      return;
    }

    const editable = canEditColumn(colIdx);
    if (!transform.informational && editable) {
      stageEdit(rowIdx, colIdx, out);
      pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: value, newValue: out }];
      futureEdits = [];
      toast.success(`${transform.label} · applied`, { duration: 2500 });
      return;
    }

    try {
      await navigator.clipboard.writeText(out);
      // Pretty-print JSON output; cap the preview so the toast stays compact.
      let preview = out;
      try { preview = JSON.stringify(JSON.parse(out), null, 2); } catch { /* not JSON */ }
      const capped = preview.length > 1200 ? preview.slice(0, 1200) + "\n…" : preview;
      toast.success(
        transform.informational ? `${transform.label} · copied` : `${transform.label} · copied, column is read-only`,
        {
          description: capped,
          code: true,
          duration: 8000,
          action: { label: "Copy again", onClick: () => navigator.clipboard.writeText(out) },
        },
      );
    } catch (e) {
      toast.error("Could not apply transform", { description: String(e?.message ?? e) });
    }
  }

  /** Stage a generated value (UUIDv7, nanoid, …) into an editable cell. */
  function insertGeneratedValue(rowIdx, colIdx, generator) {
    if (!canEditColumn(colIdx)) return;
    const prevValue = effectiveCellValue(rowIdx, colIdx);
    const next = generator.generate();
    stageEdit(rowIdx, colIdx, next);
    pastEdits = [...pastEdits.slice(-49), { rowIdx, colIdx, oldValue: prevValue, newValue: next }];
    futureEdits = [];
  }

  /** @param {number} rowIdx */
  function rowIndicesToDelete(rowIdx) {
    if (selected.size > 0 && selected.has(rowIdx)) {
      return [...selected].sort((a, b) => a - b);
    }
    return [rowIdx];
  }

  /**
   * Stage the row(s) for deletion - shown with a red diff marker until Apply.
   * Deletes are batched with edits and flushed together from the Apply button.
   * @param {number} rowIdx
   */
  function deleteRow(rowIdx) {
    if (readonly) return;
    if (!primaryKey.length) {
      toast.error("Cannot delete", {
        description: "This table has no primary key.",
      });
      return;
    }
    const rowIndices = rowIndicesToDelete(rowIdx);
    const next = new Set(pendingDeletes);
    for (const ri of rowIndices) next.add(ri);
    pendingDeletes = next;
    scheduleDraw();
  }

  /** Unstage a row previously marked for deletion. @param {number} rowIdx */
  function undoDeleteRow(rowIdx) {
    if (!pendingDeletes.has(rowIdx)) return;
    const next = new Set(pendingDeletes);
    next.delete(rowIdx);
    pendingDeletes = next;
    scheduleDraw();
  }

  /** @param {number} rowIdx */
  function duplicateRow(rowIdx) {
    if (readonly) return;
    const row = rows[rowIdx];
    if (!row) return;
    // Any oversized/truncated cell holds only a preview sentinel - duplicating it
    // would write that sentinel object into the new row. Block it and point to SQL.
    for (let i = 0; i < columns.length; i++) {
      const over = oversizeCellInfo(row[i]);
      if (over) {
        toast.error("Cannot duplicate row", {
          description: `${columns[i].name} holds ${formatByteSize(over.bytes)} and is only partially loaded; duplicate it with a SQL INSERT instead.`,
        });
        return;
      }
    }
    // The copy lands in the insert draft, not in the table. Duplicating used to
    // write immediately, which is a row in the database for one keystroke or one
    // menu click - and no chance to change the one field that made you duplicate
    // it in the first place. The draft is the same band the Add button opens, so
    // it is reviewable, editable, discardable, and goes through the same
    // confirm-and-insert path on submit.
    /** @type {Record<string, string>} */
    const drafts = {};
    columns.forEach((col, i) => {
      // A generated value and the key are the database's to assign - copying
      // them is what would make the insert collide with the row it came from.
      const auto = isAutoColumn(col, primaryKey) || primaryKey.includes(col.name);
      if (auto) { drafts[col.name] = defaultInsertDraft(col, primaryKey); return; }
      const v = row[i];
      // A json column takes JSON TEXT, and `valueToEditString` hands back the
      // bare value for anything that is not an object - so a json column holding
      // the string "sdf asdf" was copied in as sdf asdf, which is not JSON, and
      // the insert came back "answerField: Invalid JSON". Re-encode it.
      const isJsonCol = isJsonColumnType(col.dataType ?? col.data_type ?? '');
      if (isJsonCol && v !== null && v !== undefined) {
        try { drafts[col.name] = JSON.stringify(v); return } catch { /* fall through */ }
      }
      drafts[col.name] = valueToEditString(v);
    });
    newRowDrafts = [...(newRowDrafts ?? []), drafts];
    newRowFocusIdx = newRowDrafts.length - 1;
    const firstEditable = columns.find((c) => !isAutoColumn(c, primaryKey));
    newRowFocusCol = firstEditable?.name ?? columns[0]?.name ?? null;
    tableContainer?.scrollTo({ top: 0, behavior: "smooth" });
    toast.info(
      newRowDrafts.length === 1 ? "Copied into a new row" : `${newRowDrafts.length} rows staged`,
      { description: "Nothing is written until you submit them.", duration: 2600 },
    );
  }

  /** @param {number} rowIdx @param {number} colIdx @param {'down'|'right'|'left'} action @param {boolean} [autoEdit] */
  function navigateAfterEdit(rowIdx, colIdx, action, autoEdit = false) {
    _focusFromKey = true;
    const visColIdx = actualToVisColIdx(colIdx);
    const visLen = navigableColumns.length;
    const rowLen = rows.length;
    if (action === "down") {
      const next = Math.min(rowIdx + 1, rowLen - 1);
      focusedRow = next;
      focusedCol = visColIdx >= 0 ? visColIdx : 0;
      scrollRowIntoView(next);
    } else if (action === "right") {
      if (visColIdx < visLen - 1) { focusedRow = rowIdx; focusedCol = visColIdx + 1; }
      else if (rowIdx < rowLen - 1) { focusedRow = rowIdx + 1; focusedCol = 0; scrollRowIntoView(rowIdx + 1); }
    } else {
      if (visColIdx > 0) { focusedRow = rowIdx; focusedCol = visColIdx - 1; }
      else if (rowIdx > 0) { focusedRow = rowIdx - 1; focusedCol = visLen - 1; scrollRowIntoView(rowIdx - 1); }
    }
    tick().then(() => {
      if (autoEdit && focusedRow !== null && focusedCol !== null) {
        const ai = visToActualColIdx(focusedCol)
        if (ai >= 0) startEdit(focusedRow, ai)
      } else {
        tableContainer?.focus({ preventScroll: true })
      }
    })
  }

  function undoEdit() {
    if (!pastEdits.length) return;
    const last = pastEdits[pastEdits.length - 1];
    pastEdits = pastEdits.slice(0, -1);
    futureEdits = [last, ...futureEdits];
    // Undo restages the prior value (still unsaved - Apply persists it).
    stageEdit(last.rowIdx, last.colIdx, last.oldValue);
    focusedRow = last.rowIdx;
    const vi = actualToVisColIdx(last.colIdx);
    focusedCol = vi >= 0 ? vi : 0;
    scrollRowIntoView(last.rowIdx);
    tick().then(() => tableContainer?.focus({ preventScroll: true }));
  }

  function redoEdit() {
    if (!futureEdits.length) return;
    const next = futureEdits[0];
    futureEdits = futureEdits.slice(1);
    pastEdits = [...pastEdits, next];
    stageEdit(next.rowIdx, next.colIdx, next.newValue);
    focusedRow = next.rowIdx;
    const vi = actualToVisColIdx(next.colIdx);
    focusedCol = vi >= 0 ? vi : 0;
    scrollRowIntoView(next.rowIdx);
    tick().then(() => tableContainer?.focus({ preventScroll: true }));
  }

  $effect(() => {
    if (!editingCell) return;
    void tick().then(() => {
      const el = editInput;
      if (!el) return;
      el.focus();
      if (el instanceof HTMLInputElement) {
        if (selectOnEditFocus) {
          el.select();
        } else {
          const len = el.value.length;
          el.setSelectionRange(len, len);
        }
      }
    });
  });

  /** @param {KeyboardEvent} e */
  function handleEditKeydown(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancelEdit();
      return;
    }

    // Ctrl+Shift+Backspace: clear entire input
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === "Backspace") {
      e.preventDefault();
      e.stopPropagation();
      if (editingCell) editingCell.draft = "";
      return;
    }

    // Ctrl+Backspace: delete previous word (WebKit on Linux doesn't do this natively)
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key === "Backspace") {
      const el = editInput;
      if (!(el instanceof HTMLInputElement) || !editingCell) return;
      e.preventDefault();
      e.stopPropagation();
      const val = el.value;
      const pos = el.selectionStart ?? 0;
      const selEnd = el.selectionEnd ?? 0;
      if (pos !== selEnd) {
        const lo = Math.min(pos, selEnd);
        const hi = Math.max(pos, selEnd);
        editingCell.draft = val.slice(0, lo) + val.slice(hi);
        tick().then(() => { if (editInput instanceof HTMLInputElement) editInput.setSelectionRange(lo, lo); });
      } else {
        let start = pos;
        while (start > 0 && /\s/.test(val[start - 1])) start--;
        while (start > 0 && !/\s/.test(val[start - 1])) start--;
        editingCell.draft = val.slice(0, start) + val.slice(pos);
        tick().then(() => { if (editInput instanceof HTMLInputElement) editInput.setSelectionRange(start, start); });
      }
      return;
    }

    if (e.key === "Tab") {
      e.preventDefault();
      void commitEditWithAction(e.shiftKey ? "left" : "right", true);
      return;
    }
    // Alt+Enter saves this cell straight to the database, bypassing
    // the staged Apply/Reset queue.
    if (e.altKey && e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void commitEditImmediate("down");
      return;
    }
    // Enter (or Ctrl/Cmd+Enter) confirms the edit into the staged queue and
    // moves to the next row.
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      void commitEditWithAction("down");
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      void commitEditWithAction(null);
    }
  }

  /** Set of every row index, built without the intermediate array that
   *  new Set(rows.map(...)) allocates - that doubled the peak spike on huge tables. */
  function allRowIndexSet() {
    const s = new Set();
    for (let i = 0; i < rows.length; i++) s.add(i);
    return s;
  }

  function toggleAll(checked) {
    selected = checked ? allRowIndexSet() : new Set();
    lastSelectAnchor = null;
  }

  function toggleRow(idx) {
    const next = new Set(selected);
    if (next.has(idx)) next.delete(idx);
    else next.add(idx);
    selected = next;
  }

  /** Last row index clicked without Shift - the anchor for range selection. */
  let lastSelectAnchor = $state(/** @type {number | null} */ (null));

  /**
   * @param {number} idx
   * @param {boolean} shiftKey
   */
  function handleRowSelect(idx, shiftKey) {
    if (shiftKey && lastSelectAnchor !== null) {
      const lo = Math.min(lastSelectAnchor, idx);
      const hi = Math.max(lastSelectAnchor, idx);
      const next = new Set(selected);
      for (let i = lo; i <= hi; i++) next.add(i);
      selected = next;
    } else {
      toggleRow(idx);
      lastSelectAnchor = idx;
    }
  }

  /** @param {number} rowIdx */
  function isRowExpanded(rowIdx) {
    return expandedRows.has(rowIdx);
  }

  /** @param {number} rowIdx */
  function toggleRowExpand(rowIdx) {
    const next = new Set(expandedRows);
    if (next.has(rowIdx)) {
      next.delete(rowIdx);
      // Collapse: drop the cached height here too. In scaled mode the panel may
      // already be unmounted (scrolled out of the render window), so its
      // trackExpandHeight destroy() won't fire to clean up.
      if (expandedRowHeights.has(rowIdx)) {
        const h = new Map(expandedRowHeights); h.delete(rowIdx); expandedRowHeights = h;
      }
    } else {
      next.add(rowIdx);
      // Opening JSON expand: close FK sub-view for the same row (mutually exclusive)
      if (fkSubview?.rowIdx === rowIdx) fkSubview = null
    }
    expandedRows = next;
  }

  /** Collapse every expanded row at once. */
  function collapseAllRows() {
    if (expandedRows.size === 0) return;
    expandedRows = new Set();
    if (expandedRowHeights.size > 0) expandedRowHeights = new Map();
  }

  // ── Column reorder (display-only) ──────────────────────────────────────────
  // Rows are position-indexed arrays and cells resolve by column *name* (see
  // _nameToActualIdx), so reordering the visible-column list moves NOTHING in the
  // row data - it's a pure layout change and stays O(visible cols). `columnOrder`
  // is the display order of column names; names absent from it keep their natural
  // order after the ordered ones. Persisted per table.
  let columnOrder = $state(/** @type {string[]} */ ([]));
  const _colOrderKey = $derived(`stroke:colorder:${connectionId}\x00${schema}\x00${tableName}`);
  $effect(() => {
    const key = _colOrderKey;
    untrack(() => {
      try {
        const raw = localStorage.getItem(key);
        const parsed = raw ? JSON.parse(raw) : [];
        columnOrder = Array.isArray(parsed) ? parsed : [];
      } catch { columnOrder = []; }
    });
  });
  function persistColumnOrder() {
    try { localStorage.setItem(_colOrderKey, JSON.stringify(columnOrder)); } catch {}
  }
  /**
   * Move a column within the visible display order.
   * @param {string} name @param {'left'|'right'|'first'|'last'} where
   */
  function moveColumn(name, where) {
    const order = visibleColumns.map((c) => c.name);
    const from = order.indexOf(name);
    if (from < 0) return;
    order.splice(from, 1);
    const to =
      where === 'first' ? 0 :
      where === 'last'  ? order.length :
      where === 'left'  ? Math.max(0, from - 1) :
      /* right */         Math.min(order.length, from + 1);
    order.splice(to, 0, name);
    columnOrder = order;
    persistColumnOrder();
  }
  function resetColumnOrder() {
    columnOrder = [];
    persistColumnOrder();
  }

  const visibleColumns = $derived.by(() => {
    const vis = columns.filter((c) => !hiddenColumns.has(c.name));
    if (columnOrder.length === 0) return vis;
    const pos = new Map(columnOrder.map((n, i) => [n, i]));
    // Stable: ordered names by their position; unlisted keep original order last.
    return vis
      .map((c, i) => ({ c, i, k: pos.has(c.name) ? /** @type {number} */ (pos.get(c.name)) : Infinity }))
      .sort((a, b) => (a.k === b.k ? a.i - b.i : a.k - b.k))
      .map((e) => e.c);
  });

  // ── Column header highlight + tag ────────────────────────────────────────────
  // A persistent colour band on a column's header bar plus an optional short text
  // tag. Purely cosmetic, persisted per table, and independent of the transient
  // click-to-select highlight (selectedCols) above.
  // Muted, desaturated tones - refined "label" colours that read as intentional
  // on the dark header rather than saturated neon. (Tailwind-500 looked garish.)
  const COL_HIGHLIGHTS = /** @type {const} */ ([
    { id: 'red',    label: 'Red',    hex: '#dd8a8a' },
    { id: 'amber',  label: 'Amber',  hex: '#cbab7e' },
    { id: 'green',  label: 'Green',  hex: '#8fc4a3' },
    { id: 'blue',   label: 'Blue',   hex: '#8bb0d6' },
    { id: 'purple', label: 'Purple', hex: '#b1a2e0' },
    { id: 'pink',   label: 'Pink',   hex: '#d3a0c6' },
  ]);
  const COL_HL_MAP = new Map(COL_HIGHLIGHTS.map((h) => [h.id, h.hex]));
  /** @type {Record<string, { color?: string, tag?: string }>} */
  let colHighlights = $state({});
  const _colHlKey = $derived(`stroke:colhl:${connectionId}\x00${schema}\x00${tableName}`);
  $effect(() => {
    const key = _colHlKey;
    untrack(() => {
      try {
        const raw = localStorage.getItem(key);
        const parsed = raw ? JSON.parse(raw) : {};
        colHighlights = parsed && typeof parsed === 'object' ? parsed : {};
      } catch { colHighlights = {}; }
    });
  });
  function persistColHighlights() {
    try {
      if (Object.keys(colHighlights).length) localStorage.setItem(_colHlKey, JSON.stringify(colHighlights));
      else localStorage.removeItem(_colHlKey);
    } catch {}
  }
  /** @param {string} name @param {string|null} colorId */
  function setColHighlight(name, colorId) {
    const next = { ...colHighlights };
    const entry = { ...(next[name] ?? {}) };
    if (colorId) entry.color = colorId; else delete entry.color;
    if (entry.color || entry.tag) next[name] = entry; else delete next[name];
    colHighlights = next;
    persistColHighlights();
    scheduleDraw();
  }
  /** @param {string} name @param {string} tag */
  function setColTag(name, tag) {
    const next = { ...colHighlights };
    const entry = { ...(next[name] ?? {}) };
    const t = (tag ?? '').trim().slice(0, 24);
    if (t) entry.tag = t; else delete entry.tag;
    if (entry.color || entry.tag) next[name] = entry; else delete next[name];
    colHighlights = next;
    persistColHighlights();
    scheduleDraw();
  }
  // ── Per-column transform ─────────────────────────────────────────────────────
  // A cell transform (Decode JWT, Epoch → date, Base64 decode, …) chosen for a
  // whole column; the transformed value renders live in every cell. Persisted
  // per table, independent of highlight/tag.
  /** @type {Record<string, string>} colName → transform id */
  let colTransforms = $state({});
  const _colTfKey = $derived(`stroke:coltf:${connectionId}\x00${schema}\x00${tableName}`);
  $effect(() => {
    const key = _colTfKey;
    untrack(() => {
      try {
        const raw = localStorage.getItem(key);
        const parsed = raw ? JSON.parse(raw) : {};
        colTransforms = parsed && typeof parsed === 'object' ? parsed : {};
      } catch { colTransforms = {}; }
    });
  });
  function persistColTransforms() {
    try {
      if (Object.keys(colTransforms).length) localStorage.setItem(_colTfKey, JSON.stringify(colTransforms));
      else localStorage.removeItem(_colTfKey);
    } catch {}
  }
  /** @param {string} name @param {string|null} id */
  function setColTransform(name, id) {
    const next = { ...colTransforms };
    if (id) next[name] = id; else delete next[name];
    colTransforms = next;
    persistColTransforms();
    scheduleDraw();
  }
  // Active transforms resolved to their run fns (+ bound column type/name).
  const _colTransformFns = $derived.by(() => {
    void $pluginState;
    /** @type {Record<string, { id: string, run: Function, colType: string, name: string }>} */
    const out = {};
    for (const [name, id] of Object.entries(colTransforms)) {
      const t = transformById(id);
      if (!t) continue;
      const ai = _nameToActualIdx.get(name) ?? -1;
      out[name] = { id: t.id, run: t.run, colType: _colCache[ai]?.colType ?? '', name };
    }
    return out;
  });
  /** Transformed display text for a column-transformed cell, cached per row index
   *  (the cache itself + its invalidation live with the other display caches). */
  function colTransformText(/** @type {number} */ idx, /** @type {number} */ actualIdx, /** @type {unknown} */ value, /** @type {{id:string,run:Function,colType:string,name:string}} */ tf) {
    if (value === null || value === undefined) return '';
    let m = _colTfCache.get(idx);
    if (!m) {
      if (_colTfCache.size >= _DISP_CACHE_MAX) _colTfCache.clear();
      m = new Map(); _colTfCache.set(idx, m);
    }
    const k = actualIdx + ':' + tf.id;
    let cached = m.get(k);
    if (cached === undefined) {
      try { cached = displayCell(String(tf.run(value, tf.colType, tf.name))); }
      catch { cached = displayCell(value); }
      m.set(k, cached);
    }
    return cached;
  }

  // NULL / empty / whitespace markers - empty & whitespace are handled by the
  // formatter; NULL (∅) is drawn here since formatters skip null cells.
  const _nullishOn = $derived.by(() => { void $pluginState; return isPluginEnabled('nullish-values'); });

  // ── Image / avatar cell thumbnails ───────────────────────────────────────────
  // When a column's transform is avatar / image-thumb, the cell renders the image
  // instead of text. Source images are routinely multi-megapixel, and a decoded
  // 4000×3000 JPEG costs ~48 MB of RGBA regardless of how small it is drawn - so
  // the loader is deliberately stingy:
  //   • only urls painted in the current frame are ever fetched (see _imgWanted),
  //   • at most _IMG_MAX_INFLIGHT decode at a time, the rest queue,
  //   • each one is downscaled to a single _IMG_THUMB square and the full-res
  //     decode is dropped immediately,
  //   • the thumbnail cache evicts LRU-style instead of being flushed wholesale.
  // Without the in-flight cap, switching a 173-row column to image-thumb starts
  // 173 full-res decodes in the same frame, which is what made it crawl.
  const _IMG_TF = new Set(['avatar', 'image-thumb']);
  /** Ready thumbnails / permanent failures, oldest insertion first. @type {Map<string, ImageBitmap | HTMLCanvasElement | 'error'>} */
  const _imgCache = new Map();
  /** @type {Map<string, number>} per-url transient-failure retry counter */
  const _imgRetry = new Map();
  /** Urls waiting out a retry backoff - not cached, not queued. @type {Set<string>} */
  const _imgBackoff = new Set();
  /** Pending retry timers, so a destroyed table cannot fire them. @type {Set<number>} */
  const _imgTimers = new Set();
  /** Urls decoding right now, mapped to the element to abort on teardown. @type {Map<string, HTMLImageElement>} */
  const _imgLoading = new Map();
  /** Urls waiting for a free decode slot, oldest first. @type {string[]} */
  let _imgQueue = [];
  /** Urls painted in the frame being drawn; everything else is off-screen. @type {Set<string>} */
  const _imgWanted = new Set();
  const _IMG_CACHE_MAX = 300;
  const _IMG_MAX_RETRY = 3;
  const _IMG_THUMB = 128;
  const _IMG_MAX_INFLIGHT = 4;

  /**
   * Center-crop and downscale a loaded image onto a small canvas. Used when
   * createImageBitmap is unavailable or refuses the image - it rejects
   * cross-origin sources that were not served with CORS headers, and canvas
   * drawing has no such restriction. Falling back here (rather than caching the
   * full-res element) is what keeps a remote image column from retaining
   * hundreds of megabytes of decoded pixels.
   * @param {HTMLImageElement} img
   * @returns {HTMLCanvasElement | 'error'}
   */
  function makeThumbCanvas(img, sx, sy, s) {
    const cv = document.createElement('canvas');
    cv.width = _IMG_THUMB; cv.height = _IMG_THUMB;
    const cx = cv.getContext('2d');
    if (!cx) return 'error';
    cx.imageSmoothingEnabled = true;
    cx.imageSmoothingQuality = 'high';
    cx.drawImage(img, sx, sy, s, s, 0, 0, _IMG_THUMB, _IMG_THUMB);
    return cv;
  }

  /**
   * Store a finished thumbnail, then trim the cache back to its ceiling. Entries
   * are evicted oldest-first but on-screen urls are always kept, so scrolling
   * through more images than fit in the cache can never evict the rows the user
   * is actually looking at (the old code flushed the whole map, which made every
   * visible thumbnail reload at once).
   * @param {string} url
   * @param {ImageBitmap | HTMLCanvasElement | 'error'} val
   */
  function setCellImage(url, val) {
    _imgCache.set(url, val);
    if (_imgCache.size <= _IMG_CACHE_MAX) return;
    for (const k of _imgCache.keys()) {
      if (_imgCache.size <= _IMG_CACHE_MAX) break;
      if (_imgWanted.has(k)) continue;
      const v = _imgCache.get(k);
      if (typeof ImageBitmap !== 'undefined' && v instanceof ImageBitmap) v.close?.();
      _imgCache.delete(k);
    }
  }

  /**
   * Begin one decode. Remote avatar CDNs (lh3.googleusercontent.com,
   * avatars.githubusercontent.com) throttle bursts of parallel requests and
   * occasionally 403 on the app-origin Referer, so a single onerror is usually
   * transient - retry with backoff before giving up rather than freezing the
   * cell as "broken image" permanently.
   * @param {string} url
   */
  function startCellImage(url) {
    const img = new Image();
    // These CDNs reject/deny requests that carry tauri://localhost as Referer;
    // no-referrer strips it and never causes an otherwise-good load to fail.
    img.referrerPolicy = 'no-referrer';
    img.decoding = 'async';
    // Release the full-res decode as soon as the thumbnail exists, and hand the
    // freed slot to the next queued url.
    const finish = () => {
      img.onload = null; img.onerror = null;
      _imgLoading.delete(url);
      img.src = '';
      scheduleDraw();
      pumpCellImages();
    };
    img.onload = () => {
      _imgRetry.delete(url);
      const s = Math.min(img.naturalWidth, img.naturalHeight);
      if (s <= 0) { setCellImage(url, 'error'); finish(); return; }
      const sx = Math.floor((img.naturalWidth - s) / 2);
      const sy = Math.floor((img.naturalHeight - s) / 2);
      if (typeof createImageBitmap === 'function') {
        createImageBitmap(img, sx, sy, s, s, { resizeWidth: _IMG_THUMB, resizeHeight: _IMG_THUMB, resizeQuality: 'high' })
          .then((bmp) => { setCellImage(url, bmp); })
          .catch(() => { setCellImage(url, makeThumbCanvas(img, sx, sy, s)); })
          .finally(finish);
        return;
      }
      setCellImage(url, makeThumbCanvas(img, sx, sy, s));
      finish();
    };
    img.onerror = () => {
      img.onload = null; img.onerror = null;
      _imgLoading.delete(url);
      img.src = '';
      const n = (_imgRetry.get(url) || 0) + 1;
      if (n <= _IMG_MAX_RETRY) {
        _imgRetry.set(url, n);
        // Hold the url out of the queue while it backs off, otherwise the next
        // repaint re-requests it immediately and the backoff means nothing.
        // Stagger so a whole column does not re-fire at once.
        _imgBackoff.add(url);
        const t = setTimeout(() => {
          _imgTimers.delete(t);
          _imgBackoff.delete(url);
          if (_imgWanted.has(url)) { _imgQueue.push(url); pumpCellImages(); }
        }, 350 * n);
        _imgTimers.add(t);
      } else {
        setCellImage(url, 'error');
      }
      scheduleDraw();
      pumpCellImages();
    };
    _imgLoading.set(url, img);
    img.src = url;
  }

  /**
   * Drain the queue up to the in-flight cap. Called once per frame after the
   * visible set has been rebuilt, so a fast scroll drops the backlog it flew
   * past instead of decoding rows that are long gone.
   */
  function pumpCellImages() {
    if (_imgQueue.length) _imgQueue = _imgQueue.filter((u) => _imgWanted.has(u));
    while (_imgQueue.length && _imgLoading.size < _IMG_MAX_INFLIGHT) {
      const next = _imgQueue.shift();
      if (next !== undefined) startCellImage(next);
    }
  }

  /** Drop every cached/in-flight image. Called on teardown. */
  function releaseCellImages() {
    for (const v of _imgCache.values()) {
      if (typeof ImageBitmap !== 'undefined' && v instanceof ImageBitmap) v.close?.();
    }
    _imgCache.clear();
    for (const img of _imgLoading.values()) { img.onload = null; img.onerror = null; img.src = ''; }
    _imgLoading.clear();
    for (const t of _imgTimers) clearTimeout(t);
    _imgTimers.clear();
    _imgRetry.clear();
    _imgBackoff.clear();
    _imgQueue = [];
    _imgWanted.clear();
  }

  /**
   * Thumbnail for a url if one is ready. Never starts a decode itself - the draw
   * path only registers interest, and pumpCellImages decides what actually runs.
   * @param {string} url
   * @returns {ImageBitmap | HTMLCanvasElement | 'error' | null}
   */
  function getCellImage(url) {
    _imgWanted.add(url);
    const hit = _imgCache.get(url);
    if (hit !== undefined) return hit;
    if (!_imgLoading.has(url) && !_imgBackoff.has(url) && !_imgQueue.includes(url)) _imgQueue.push(url);
    return null;
  }

  /** Draw an image/avatar thumbnail (center-cropped) with a filename beside it. */
  function drawCellImage(ctx, url, cellX, ry, w, rh, cy, round, c) {
    const size = Math.min(rh - Math.round(6 * canvasZoom), Math.round(26 * canvasZoom));
    const ix = cellX + CELL_PAD_X;
    const iy = Math.round(cy - size / 2);
    const radius = round ? size / 2 : Math.round(4 * canvasZoom);
    const thumb = getCellImage(url);
    const ready = thumb !== null && thumb !== 'error';
    roundRect(ctx, ix, iy, size, size, radius);
    ctx.fillStyle = withAlpha(c.cMutedBg, 0.5); ctx.fill();
    if (ready) {
      ctx.save();
      roundRect(ctx, ix, iy, size, size, radius); ctx.clip();
      // Already a center-cropped square thumbnail - cheap blit, no per-frame resample.
      ctx.drawImage(/** @type {CanvasImageSource} */ (thumb), ix, iy, size, size);
      ctx.restore();
    }
    ctx.strokeStyle = withAlpha(c.cBorder, 0.6); ctx.lineWidth = 1;
    roundRect(ctx, ix, iy, size, size, radius); ctx.stroke();
    ctx.font = _fonts.cell; ctx.textAlign = 'left';
    ctx.fillStyle = c.cMuted;
    const label = thumb === 'error' ? 'broken image' : ready ? (url.split('/').pop() || url) : 'loading…';
    const lx = ix + size + Math.round(8 * canvasZoom);
    ctx.fillText(truncText(ctx, label, Math.max(0, cellX + w - 4 - lx)), lx, cy + 0.5);
  }

  // Tag input dialog
  let tagDialogOpen = $state(false);
  let tagDialogCol = $state('');
  let tagDialogValue = $state('');
  /** @param {string} name */
  function openTagDialog(name) {
    tagDialogCol = name;
    tagDialogValue = colHighlights[name]?.tag ?? '';
    tagDialogOpen = true;
  }
  function confirmTag() {
    if (tagDialogCol) setColTag(tagDialogCol, tagDialogValue);
    tagDialogOpen = false;
  }

  // ── Virtual relationship columns (reverse FK / one-to-many) ─────────────────
  // One per unique fromTable, max 8. Shown as badge columns to the right of real data.
  const MAX_VIRTUAL_COLS = 5
  /** Per-column logical width overrides for virtual rel columns, keyed by label (unset = auto). */
  let _vrelWidths = $state(/** @type {Record<string, number>} */ ({}))
  // Auto width adapts to the longest label (8px/char estimate + padding), clamped 150-260px.
  const _vrelBaseW = $derived.by(() => {
    if (!virtualRelCols.length) return 200
    const maxChars = Math.max(...virtualRelCols.map(v => v.label.length))
    return Math.min(260, Math.max(150, maxChars * 8 + 44))
  })
  const virtualRelCols = $derived.by(() => {
    if (!incomingForeignKeys.length) return /** @type {typeof incomingForeignKeys} */ ([])
    const seen = new Set()
    const result = []
    for (const fk of incomingForeignKeys) {
      if (seen.has(fk.fromTable)) continue
      seen.add(fk.fromTable)
      const label = (fk.fromSchema && fk.fromSchema !== schema) ? `${fk.fromSchema}.${fk.fromTable}` : fk.fromTable
      if (hiddenColumns.has(`__vrel:${label}`)) continue
      result.push({ ...fk, label })
      if (result.length >= MAX_VIRTUAL_COLS) break
    }
    return result
  })
  // ── Virtual expression columns (user-defined templates) ──────────────────────
  const VEXPR_COL_DEFAULT_W = 220
  const _tableKey = $derived(`${connectionId}\x00${schema}.${tableName}`)
  /** Active (enabled) virtual expr col defs for current table */
  const _vcols = $derived.by(() => {
    const all = $virtualColumnsStore[_tableKey] ?? []
    return all.filter(c => c.enabled)
  })
  /**
   * Bound evaluator functions - compiled once when columns change, not per-render.
   * Each fn takes a row array and returns the computed string.
   */
  const _vcolFns = $derived.by(() => {
    const nameToIdx = _nameToActualIdx
    return _vcols.map(vc => bindExpr(vc.expression, nameToIdx))
  })
  /** Canvas-space x/w layout for each active vexpr col */
  const _vexprLayout = $derived.by(() => {
    let x = geom.totalWidth
    return _vcols.map((vc, i) => {
      const w = Math.round((_vexprWidths[vc.id] ?? VEXPR_COL_DEFAULT_W) * canvasZoom)
      // hoverKey is precomputed here so the per-row draw loop never builds the
      // `__vcol__${id}` string per cell per frame (GC churn on the scroll path).
      const pos = { id: vc.id, name: vc.name, x, w, fnIdx: i, hoverKey: `__vcol__${vc.id}` }
      x += w
      return pos
    })
  })
  /** Precomputed `__vrel__N` hover keys - same per-frame allocation avoidance. */
  const _vrelHoverKeys = $derived(virtualRelCols.map((_, i) => `__vrel__${i}`))
  const vexprTotalW = $derived(
    _vexprLayout.length > 0
      ? _vexprLayout[_vexprLayout.length - 1].x + _vexprLayout[_vexprLayout.length - 1].w - geom.totalWidth
      : 0
  )
  /** Canvas-space x/w layout for each virtual rel column - per-column widths so
   *  resizing one doesn't resize the rest. x is absolute (right of real + expr cols). */
  const _vrelLayout = $derived.by(() => {
    let x = geom.totalWidth + vexprTotalW
    return virtualRelCols.map((vc, i) => {
      const w = Math.round((_vrelWidths[vc.label] ?? _vrelBaseW) * canvasZoom)
      const pos = { i, label: vc.label, x, w, hoverKey: _vrelHoverKeys[i] }
      x += w
      return pos
    })
  })
  const vrelTotalW = $derived(
    _vrelLayout.length > 0
      ? _vrelLayout[_vrelLayout.length - 1].x + _vrelLayout[_vrelLayout.length - 1].w - geom.totalWidth - vexprTotalW
      : 0
  )

  // Total scrollable width includes virtual expr cols + virtual rel columns
  const totalContentWidth = $derived(geom.totalWidth + vexprTotalW + vrelTotalW)
  // Surface whether the grid overflows horizontally so the parent can show the
  // go-to-left / go-to-right controls only when they'd actually do something.
  $effect(() => {
    canScrollHorizontally = totalContentWidth > _viewportWidth + 1
  })
  /**
   * Where a staged-row cell sits inside the band's scrolling layer.
   *
   * Every x and every width in the band comes from `geom`, the same object the
   * canvas draws from, because a second sum of the same widths drifts the
   * moment one of the two reads something the other does not. This one used to
   * add up the gutters and the columns by hand and left the row-number gutter
   * out, so every staged cell sat that gutter's width to the left of the column
   * it belonged to - which is the band and the grid sliding past each other
   * when you drag a wide table sideways.
   *
   * The layer is translated by `-_scrollLeft`, so an ordinary column sits at its
   * content x and the transform carries it. A pinned column has to undo that
   * shift once it reaches its frozen slot: `colDrawnX` already works that out
   * for the canvas, and adding the scroll back puts its answer in the layer's
   * coordinates.
   * @param {{ name: string, contentX: number, w: number, pinned: boolean }} col
   */
  function bandCellX(col) {
    return col.pinned ? colDrawnX(col, geom, _scrollLeft) + _scrollLeft : col.contentX
  }

  /**
   * Column order for the cell cursor.
   *
   * Relationship columns are part of it. They are cells you can act on - Enter
   * opens the related rows in the dock, the same as a click - and leaving them
   * out meant Tab walked to the last real column and stopped, with the one
   * column that opens something unreachable without the mouse. They carry the
   * same `__vrel__i` key the hover path already uses, so nothing has to guess
   * whether a name belongs to a real column: `visToActualColIdx` returns -1 for
   * them, which every edit path already treats as "not editable".
   */
  const navigableColumns = $derived(
    virtualRelCols.length
      // Keyed off the hover keys, not the layout: the cursor's column order has
      // no business recomputing every time a zoom or a drag changes an x.
      ? [...visibleColumns, ...virtualRelCols.map((_, i) => ({ name: _vrelHoverKeys[i], vrelIdx: i }))]
      : visibleColumns,
  )

  /** The relationship column a navigable index points at, or null for a real one. */
  function vrelAtVisIdx(visColIdx) {
    const idx = navigableColumns[visColIdx]?.vrelIdx
    return idx === undefined ? null : (virtualRelCols[idx] ?? null)
  }

  // ── Accessibility: focused-cell announcement ────────────────────────────────
  // The canvas grid has no per-cell DOM, so screen readers get nothing on
  // navigation. This derived builds a short description of the focused cell and
  // is rendered into an aria-live region. It depends ONLY on focus/edit state and
  // the data - NOT on scroll offsets - so it never recomputes during the rAF draw
  // loop and cannot affect render throughput.
  const a11yCellAnnouncement = $derived.by(() => {
    if (focusedRow === null || focusedCol === null) return ''
    const vrel = vrelAtVisIdx(focusedCol)
    if (vrel) return `Row ${focusedRow + 1} of ${rows.length}, related ${vrel.label}, press Enter to open`
    const ai = visToActualColIdx(focusedCol)
    if (ai < 0) return ''
    const col = columns[ai]
    const row = rows[focusedRow]
    if (!col || !row) return ''
    const raw = formatCell(row[ai])
    const val = raw.length > 80 ? raw.slice(0, 80) + '…' : raw
    const editing = editingCell && editingCell.rowIdx === focusedRow && editingCell.colIdx === ai
    return `Row ${focusedRow + 1} of ${rows.length}, ${col.name}: ${val}${editing ? ', editing' : ''}`
  })
  // ── Canvas geometry (single source of truth for draw + hit-test) ───────────
  const gutterWidth = $derived(
    (showRowExpand ? GUTTER_EXPAND_W : 0) + (showSelection ? GUTTER_SELECT_W : 0) + GUTTER_NUM_W,
  )
  const geom = $derived.by(() => {
    const cols = visibleColumns.map((c) => ({ name: c.name, dataType: c.dataType ?? c.data_type ?? '' }))
    // The type matters: `widthForColumn` falls back to the type's default width
    // whenever a column has no stored width yet, which is every column for the
    // frame between a table switch and the effect that seeds `columnWidths`.
    // Passing '' here and the real type elsewhere made those two callers size
    // the same column differently for that frame.
    const typeOf = new Map(cols.map((c) => [c.name, c.dataType]))
    return computeColumnGeometry({
      columns: cols,
      widthOf: (name) => widthForColumn(name, typeOf.get(name) ?? ''),
      isPinned: (name) => pinnedColumns.has(name),
      gutterWidth,
    })
  })
  // FK sub-view is a zero-cost overlay - it does NOT push rows down and is NOT
  // included in rowTops. This eliminates the fkSubviewHeight→_mergedHeights→rowTops
  // reactive chain that caused lag every time the panel opened or changed height.
  // null when no row is expanded (the common case) → rowDocTop/rowIndexAtY use
  // O(1) `idx * ROW_HEIGHT` math and no per-page Float64Array is allocated.
  const rowTops = $derived(computeRowTops(rows.length, expandedRows, 280, ROW_HEIGHT, expandedRowHeights))
  /**
   * Total scrollable content height: header + insert slot + body, plus a 2-row
   * bottom margin so the last row can be scrolled clear of the viewport edge.
   *
   * Embedded results get no margin. They are height-capped, not filled, so the
   * margin never earns its keep there - it just left two empty row-heights
   * hanging under a short result (a one-row COUNT looked like a broken table).
   */
  const contentHeight = $derived(
    HEADER_H + insertRowOffset + totalRowsHeight(rowTops, rows.length, ROW_HEIGHT) +
    (embedded ? 0 : ROW_HEIGHT * 2),
  )

  // Browser engines cap element/scroll height at ~33.5M px, so a naive spacer of
  // rows×ROW_HEIGHT breaks past ~1.4M rows (rows become unreachable). When the
  // true content is taller than this, the spacer is capped and the scroll range
  // is compressed: physical scrollTop 0…(cap−vh) maps onto virtual 0…(natural−vh)
  // by `_scrollScale`. Below the cap (the overwhelming common case) scale is 1 and
  // everything behaves exactly as an un-normalized native scroll.
  const MAX_SCROLL_PX = 24_000_000
  const spacerHeight = $derived(Math.min(contentHeight, MAX_SCROLL_PX))
  const _scrollScale = $derived.by(() => {
    if (contentHeight <= MAX_SCROLL_PX) return 1
    const physRange = Math.max(1, MAX_SCROLL_PX - _viewportHeight)
    const virtRange = Math.max(0, contentHeight - _viewportHeight)
    return virtRange > physRange ? virtRange / physRange : 1
  })
  /** Physical DOM scrollTop → virtual content offset (row math). */
  function physToVirt(/** @type {number} */ phys) {
    if (_scrollScale === 1) return phys
    return Math.round(Math.min(Math.max(0, contentHeight - _viewportHeight), phys * _scrollScale))
  }
  /** Virtual content offset → physical DOM scrollTop (for programmatic scrolls). */
  function virtToPhys(/** @type {number} */ virt) {
    return _scrollScale === 1 ? virt : virt / _scrollScale
  }
  /** Scroll the container so virtual offset `virt` sits at the top. */
  function setVirtualScroll(/** @type {number} */ virt) {
    if (tableContainer) tableContainer.scrollTop = Math.max(0, virtToPhys(virt))
  }

  /** True while the scroll rAF loop is live - gates DOM-overlay work that would
   *  otherwise re-render every scroll frame (resize handles reposition per frame
   *  via keyed style writes; nobody can grab one mid-scroll anyway). */
  let _isScrolling = $state(false)
  const _EMPTY_HANDLES = /** @type {{ name: string, x: number }[]} */ ([])

  /** Viewport-visible column resize handles (DOM overlay - not on the canvas). */
  const resizeHandles = $derived.by(() => {
    if (_isScrolling) return _EMPTY_HANDLES
    /** @type {{ name: string, x: number }[]} */
    const out = []
    // Handles hidden behind the frozen pinned region are dropped - except the
    // pinned columns' own edges, which are what defines that region.
    const occludeLeft = geom.frozenWidth
    // A handle is centred on its column's right edge and is 10px wide, so the
    // last column's handle lands half outside the viewport as soon as the table
    // is scrolled to its end - and the sliver that remains sits under the
    // vertical scrollbar. The last column could not be resized at all. Every
    // handle is pulled far enough in to keep its whole target reachable; at 7px
    // the grab zone still overlaps the edge it resizes.
    const maxX = _viewportWidth - 7
    const place = (/** @type {number} */ x) => Math.min(x, maxX)
    for (const col of geom.cols) {
      const x = colDrawnX(col, geom, _scrollLeft) + col.w
      if ((!col.pinned && x < occludeLeft - 6) || x > _viewportWidth + 6) continue
      out.push({ name: col.name, x: place(x) })
    }
    // Virtual expr column resize handles
    for (const vc of _vexprLayout) {
      const x = vc.x + vc.w - _scrollLeft
      if (x < occludeLeft - 6 || x > _viewportWidth + 6) continue
      out.push({ name: `__vcol__${vc.id}`, x: place(x) })
    }
    // Virtual rel column resize handles (right edge of each virtual col)
    for (const vp of _vrelLayout) {
      const x = vp.x + vp.w - _scrollLeft
      if (x < occludeLeft - 6 || x > _viewportWidth + 6) continue
      out.push({ name: `__vrel__${vp.i}`, x: place(x) })
    }
    return out
  })

  /** Document-space y of a body row's top (0 = top of the sizer). */
  function rowDocTop(/** @type {number} */ idx) {
    return HEADER_H + insertRowOffset + rowTopOf(rowTops, idx, ROW_HEIGHT)
  }
  /** Viewport y of a body row's top. */
  function rowViewportY(/** @type {number} */ idx) {
    return rowDocTop(idx) - _scrollTop
  }
  // Stable key that changes only when column names change - prevents the
  // column-widths $effect from re-running on every row fetch (same columns, new array ref).
  const _columnNamesKey = $derived(columns.map((c) => c.name).join('\x00'))

  // When the parent applies a fresh page of rows (page/filter/sort/search), jump
  // back to the top. Resetting _scrollTop in the same pre-paint flush keeps the
  // virtual window matched to the new scroll position, so the swap renders the
  // small top slice directly instead of a stale mid-table window that then
  // snaps - that snap is both the "scroll jumps" glitch and an extra re-render.
  let _firstReload = true
  $effect(() => {
    void reloadToken
    if (_firstReload) { _firstReload = false; return }
    // Only reset vertical scroll - preserve horizontal position so the user
    // stays looking at the same columns after a sort or filter reload.
    untrack(() => {
      _scrollTop = 0
      _physScrollTop = 0
      if (tableContainer && tableContainer.scrollTop !== 0) tableContainer.scrollTop = 0
      fkSubview = null
      // A fresh page of rows (page/filter/sort/search) invalidates row-index-keyed
      // staged changes - drop them and their cache entry so a later Apply can't
      // target the wrong rows.
      if (pendingEdits.size) pendingEdits = new Map()
      if (pendingDeletes.size) pendingDeletes = new Set()
      clearPendingChanges(_persistKey)
      // A fresh row set also invalidates the row-index-keyed cell range.
      if (selAnchor !== null) selAnchor = null
    })
  })

  // A tab opened by following a foreign key: the one row the key points at
  // opens as JSON the moment it lands. Once per row set, so collapsing it by
  // hand sticks until the next fetch. A set that landed while the tab was in
  // the background is handled by the per-table restore further down.
  let _autoExpandedRows = /** @type {unknown} */ (null)
  $effect(() => {
    if (!expandSingleRow || loading || rows.length !== 1) return
    const r = rows
    untrack(() => {
      if (_autoExpandedRows === r) return
      _autoExpandedRows = r
      if (expandedRows.size === 0) expandedRows = new Set([0])
    })
  })

  // Persist staged changes when the component tears down (switching to a SQL/AI
  // tab unmounts DataTable) so they survive until the user returns to the table.
  onDestroy(() => {
    savePendingChanges(_persistKey, pendingEdits, pendingDeletes)
  })

  const allSelected = $derived(
    rows.length > 0 && selected.size === rows.length,
  );
  const someSelected = $derived(
    selected.size > 0 && selected.size < rows.length,
  );
  const hasPrimaryKey = $derived(primaryKey.length > 0);

  /**
   * Per-column display metadata: pk, fk, indexed, unique, nullable.
   * Keyed by column name.
   * @type {Map<string, { pk: boolean, fk: boolean, indexed: boolean, unique: boolean, nullable: boolean }>}
   */
  // Pre-build a column→indexes lookup once so colMeta is O(1) per column
  // instead of O(columns × indexes) on every schema load.
  const _indexesByCol = $derived.by(() => {
    /** @type {Map<string, typeof indexes>} */
    const m = new Map()
    for (const idx of indexes) {
      for (const raw of idx.columns.split(',')) {
        const col = raw.trim()
        const list = m.get(col) ?? []
        list.push(idx)
        m.set(col, list)
      }
    }
    return m
  })

  const _pkSet = $derived(new Set(primaryKey))
  const _fkCols = $derived(new Set(foreignKeys.flatMap((fk) => fk.columns)))

  // Per-column stable cache - computed once per column/schema change instead of
  // once per cell per render. getColumnEnumValues, canEditColumn, and fkByColumn
  // were previously called rows×cols times on every reactive update.
  const _colCache = $derived.by(() => {
    // Alignment mode is per-table, not per-column, but it decides `alignRight`
    // below, so it belongs in this cache's dependency set.
    const align = $appTableAlign
    return columns.map((col) => {
      const colType = col?.dataType ?? col?.data_type ?? ''
      const t = String(colType)
      return {
        colType,
        enumValues: getColumnEnumValues(col),
        canEdit: !readonly && primaryKey.length > 0 && isEditableType(t),
        fk: fkByColumn[col?.name ?? ''] ?? null,
        // Type predicates. Each one is a regex over the column's declared type,
        // and drawCell needs all of them; evaluated there they ran once per cell
        // per frame - four regexes and half a dozen throwaway strings per cell,
        // ~40k regex executions/sec while scrolling. They depend only on the
        // column, so they are resolved once here instead.
        isBool: isBooleanType(t),
        isArrayType: isSqlArrayType(t),
        isVector: isVectorType(t),
        isGeom: isGeometryType(t),
        // Right-alignment under the 'numbers' setting: quantities line up by
        // place value, prose stays left.
        alignRight: columnAlignsRight(t, align),
      }
    })
  })

  // Per-column numeric stats for heatmap + annotator extensions. Computed once
  // per data/settings change (NOT per render), sampled to bound cost on big
  // infinite-scroll result sets. Returns null when no stats-dependent extension
  // is enabled, so the scan is skipped entirely in the common case.
  const STATS_SAMPLE = 5000
  const STATS_BUCKETS = 24
  const _statsNumericRe = /(int|numeric|decimal|real|double|float|money|number|serial)/i

  /**
   * Whether a column's cell text is drawn flush right, per the Appearance
   * setting. 'numbers' is the spreadsheet convention: digits line up by place
   * value so magnitudes are comparable down the column, prose stays left where
   * the eye finds the start of each line.
   * @param {number} actualIdx index into `columns`
   */
  function isRightAlignedColumn(actualIdx) {
    const cached = _colCache[actualIdx]
    if (cached) return cached.alignRight
    // No such column (a virtual/unmapped index): only the blanket 'right' mode
    // still applies.
    return $appTableAlign === 'right'
  }
  const _colStats = $derived.by(() => {
    void $pluginState
    if (!statsNeeded() && !annotatorEnabled()) return null
    const wantHist = annotatorEnabled()
    const n = Math.min(rows.length, STATS_SAMPLE)
    /** @type {Map<number, { numeric: boolean, min: number, max: number, nulls: number, total: number, hist: number[] | null }>} */
    const map = new Map()
    for (let a = 0; a < columns.length; a++) {
      const numeric = _statsNumericRe.test(String(columns[a]?.dataType ?? columns[a]?.data_type ?? ''))
      let min = Infinity, max = -Infinity, nulls = 0
      for (let r = 0; r < n; r++) {
        const v = rows[r]?.[a]
        if (v === null || v === undefined) { nulls++; continue }
        if (numeric) {
          const num = typeof v === 'number' ? v : Number(v)
          if (Number.isFinite(num)) { if (num < min) min = num; if (num > max) max = num }
        }
      }
      let hist = null
      if (wantHist && numeric && max > min) {
        hist = new Array(STATS_BUCKETS).fill(0)
        const span = max - min
        for (let r = 0; r < n; r++) {
          const v = rows[r]?.[a]
          if (v === null || v === undefined) continue
          const num = typeof v === 'number' ? v : Number(v)
          if (!Number.isFinite(num)) continue
          const b = Math.min(STATS_BUCKETS - 1, Math.floor(((num - min) / span) * STATS_BUCKETS))
          hist[b]++
        }
      }
      map.set(a, { numeric, min: numeric ? min : NaN, max: numeric ? max : NaN, nulls, total: n, hist })
    }
    return map
  })

  const colMeta = $derived.by(() => {
    /** @type {Map<string, { pk: boolean, fk: boolean, indexed: boolean, unique: boolean, nullable: boolean }>} */
    const map = new Map()
    for (const col of columns) {
      const colIndexes = _indexesByCol.get(col.name) ?? []
      // Single pass instead of two .some() - early exits once both flags are found
      let unique = false, indexed = false
      for (const idx of colIndexes) {
        if (!unique && idx.isUnique && !idx.isPrimary) unique = true
        else if (!indexed && !idx.isPrimary && !idx.isUnique) indexed = true
        if (unique && indexed) break
      }
      map.set(col.name, {
        pk: _pkSet.has(col.name),
        fk: _fkCols.has(col.name),
        unique,
        indexed,
        nullable: col.nullable !== false,
      })
    }
    return map
  })

  /** Returns the display width of a column in canvas px (logical × canvasZoom).
   *  @param {string} name @param {string} dataType */
  function widthForColumn(name, dataType) {
    const logical = columnWidths[name] ?? defaultColumnWidth(dataType)
    return Math.round(logical * canvasZoom)
  }

  $effect(() => {
    const key = columnWidthsKey
    _columnNamesKey  // re-run when column names change, but not on row fetches
    const cols = untrack(() => columns)
    const stored = key ? loadColumnWidths(key) : {}
    /** @type {Record<string, number>} */
    const next = {}
    for (const col of cols) {
      const dt = col.dataType ?? col.data_type ?? ""
      next[col.name] = clampColumnWidth(stored[col.name] ?? defaultColumnWidth(dt))
    }
    columnWidths = next
  })

  // Virtual (relationship / expression) column widths persist alongside real
  // ones, under sibling keys so the real-column effect above never clobbers
  // them. Reloaded on table switch; written on resize in endColumnResize.
  $effect(() => {
    const key = columnWidthsKey
    _vrelWidths = key ? loadColumnWidths(`${key}\x00__vrel`) : {}
    _vexprWidths = key ? loadColumnWidths(`${key}\x00__vcol`) : {}
  })

  /** @param {string} colName */
  function startColumnResize(colName) {
    resizingColName = colName;
    if (colName.startsWith('__vcol__')) {
      const id = colName.slice(8)
      resizeStartWidth = _vexprWidths[id] ?? VEXPR_COL_DEFAULT_W
    } else if (colName.startsWith('__vrel__')) {
      const label = virtualRelCols[Number(colName.slice(8))]?.label
      resizeStartWidth = (label ? _vrelWidths[label] : null) ?? _vrelBaseW
    } else {
      resizeStartWidth = columnWidths[colName] ?? defaultColumnWidth("")
    }
  }

  // Batch column resize updates to animation frames - pointermove can fire at
  // 120Hz+, but we only need to update the DOM at 60fps.
  let _resizeRafId = 0;
  let _pendingResizeWidth = 0;

  /** @param {number} dx */
  function applyColumnResize(dx) {
    if (!resizingColName) return;
    // dx is screen pixels; convert to logical (un-zoomed) before clamping
    _pendingResizeWidth = clampColumnWidth(resizeStartWidth + Math.round(dx / canvasZoom))
    if (_resizeRafId) return;
    _resizeRafId = requestAnimationFrame(() => {
      _resizeRafId = 0;
      if (!resizingColName) return;
      if (resizingColName.startsWith('__vcol__')) {
        const id = resizingColName.slice(8)
        _vexprWidths = { ..._vexprWidths, [id]: _pendingResizeWidth }
      } else if (resizingColName.startsWith('__vrel__')) {
        const label = virtualRelCols[Number(resizingColName.slice(8))]?.label
        if (label) _vrelWidths = { ..._vrelWidths, [label]: _pendingResizeWidth }
      } else {
        columnWidths = { ...columnWidths, [resizingColName]: _pendingResizeWidth };
      }
    });
  }

  function endColumnResize() {
    if (_resizeRafId) {
      cancelAnimationFrame(_resizeRafId);
      _resizeRafId = 0;
      if (resizingColName) {
        if (resizingColName.startsWith('__vcol__')) {
          const id = resizingColName.slice(8)
          _vexprWidths = { ..._vexprWidths, [id]: _pendingResizeWidth }
        } else if (resizingColName.startsWith('__vrel__')) {
          const label = virtualRelCols[Number(resizingColName.slice(8))]?.label
          if (label) _vrelWidths = { ..._vrelWidths, [label]: _pendingResizeWidth }
        } else {
          columnWidths = { ...columnWidths, [resizingColName]: _pendingResizeWidth };
        }
      }
    }
    if (columnWidthsKey && resizingColName) {
      if (resizingColName.startsWith('__vrel__')) saveColumnWidths(`${columnWidthsKey}\x00__vrel`, _vrelWidths);
      else if (resizingColName.startsWith('__vcol__')) saveColumnWidths(`${columnWidthsKey}\x00__vcol`, _vexprWidths);
      else saveColumnWidths(columnWidthsKey, columnWidths);
    }
    resizingColName = null;
  }

  /** Current sort keys in priority order: primary (rowSort) then secondary. */
  function currentSortList() {
    return (rowSort ? [rowSort] : []).concat(rowSortMore ?? [])
  }

  /** colName → { direction, index, total } for header sort arrows + rank badges. */
  const _sortLookup = $derived.by(() => {
    const m = new Map()
    const list = (rowSort ? [rowSort] : []).concat(rowSortMore ?? [])
    list.forEach((s, i) => { if (s?.column) m.set(s.column, { direction: s.direction, index: i, total: list.length }) })
    return m
  })

  /** True while there are unsaved edits (sorting would reorder rows and desync
   *  the row-index-keyed staged changes). Warns the user. */
  function blockedBySort() {
    if (pendingEdits.size > 0) {
      toast.error("Unsaved changes", { description: "Apply or reset your edits before sorting." })
      return true
    }
    return false
  }

  /**
   * @param {string} colName
   * @param {boolean} [additive] shift-click: add/toggle this as a SECONDARY key
   *   instead of replacing the sort - enables multi-column sort.
   */
  function handleHeaderSort(colName, additive = false) {
    if (blockedBySort()) return
    if (additive) {
      const cur = currentSortList()
      const idx = cur.findIndex((s) => s.column === colName)
      let next
      if (idx === -1) next = [...cur, { column: colName, direction: 'asc' }]
      else if (cur[idx].direction === 'asc') next = cur.map((s, i) => i === idx ? { ...s, direction: 'desc' } : s)
      else next = cur.filter((_, i) => i !== idx) // asc → desc → remove
      onsortchange(next)
      return
    }
    // Plain click: single-key cycle none → desc → asc → none.
    if (rowSort?.column !== colName) onsortchange([{ column: colName, direction: 'desc' }])
    else if (rowSort.direction === 'desc') onsortchange([{ column: colName, direction: 'asc' }])
    else onsortchange([])
  }

  /** Toggle pinning a column to the left. */
  function toggleColumnPin(colName) {
    const next = new Set(pinnedColumns)
    if (next.has(colName)) next.delete(colName)
    else next.add(colName)
    pinnedColumns = next
  }

  /** Sort by a column with an explicit direction, guarding against pending edits. */
  function headerSortDirect(colName, /** @type {'asc' | 'desc'} */ dir) {
    if (blockedBySort()) return
    onsortchange([{ column: colName, direction: dir }])
  }

  /** Reset a column's width to its default. */
  function resetColumnWidth(colName) {
    const col = columns.find((c) => c.name === colName)
    const dt = col?.dataType ?? col?.data_type ?? ''
    columnWidths = { ...columnWidths, [colName]: clampColumnWidth(defaultColumnWidth(dt)) }
    if (columnWidthsKey) saveColumnWidths(columnWidthsKey, columnWidths)
  }

  // ── Per-tab expand/sub-view state preservation ───────────────────────────────
  // Expand rows and FK sub-view are saved per columnWidthsKey so switching tabs
  // restores exactly what the user had open in each table.
  /** @type {Map<string, { expandedRows: Set<number>, expandedRowHeights: Map<number, number>, fkSubview: typeof fkSubview, newRowDrafts: Record<string, string>[] | null, newRowFocusCol: string | null }>} */
  const _tabExpandCache = new Map()
  // Cap the per-tab cache: each entry can retain a whole FK sub-view's fetched
  // rows, so an unbounded map would accumulate row data for every table visited
  // in a session. LRU-evict the oldest once over the cap (Map keeps insertion
  // order; re-inserting on save moves an entry to the most-recent position).
  const TAB_EXPAND_CACHE_MAX = 12
  let _lastTabKey = $state(untrack(() => columnWidthsKey ?? ''))

  $effect(() => {
    const newKey = columnWidthsKey ?? ''
    if (newKey === _lastTabKey) return
    untrack(() => {
      // Save state for the tab we're leaving
      if (_lastTabKey !== undefined && _lastTabKey !== '') {
        _tabExpandCache.delete(_lastTabKey) // re-insert at MRU position
        _tabExpandCache.set(_lastTabKey, {
          expandedRows: new Set(expandedRows),
          // The measured panel heights travel with the rows. Without them a
          // restored panel started at the 280px placeholder and the rows under
          // it jumped when it measured - the flick on every switch back.
          expandedRowHeights: new Map(expandedRowHeights),
          fkSubview: fkSubview,
          // The draft belongs to the table it was opened on. One component serves
          // every tab, so without this the Add row you started in one table was
          // sitting in the next one you switched to, over a different set of
          // columns.
          //
          // An UNTOUCHED draft is not kept. It costs a row of height, so every
          // tab switch with one open shifted the rows under it - a layout change
          // for something nobody had typed into yet. A draft with anything in it
          // is work, and work is kept.
          newRowDrafts: draftHasContent(newRowDrafts) ? newRowDrafts?.map((d) => ({ ...d })) ?? null : null,
          newRowFocusCol: draftHasContent(newRowDrafts) ? newRowFocusCol : null,
        })
        // Evict least-recently-used entries (oldest insertion order) over the cap.
        while (_tabExpandCache.size > TAB_EXPAND_CACHE_MAX) {
          const oldest = _tabExpandCache.keys().next().value
          if (oldest === undefined) break
          _tabExpandCache.delete(oldest)
        }
        // Preserve unsaved edits/deletes for the table we're leaving so they're
        // restored when the user returns instead of being silently discarded.
        savePendingChanges(_lastTabKey, pendingEdits, pendingDeletes)
      }
      // Restore state for the tab we're entering (fresh Set/null if first visit)
      const saved = _tabExpandCache.get(newKey)
      expandedRows = saved ? new Set(saved.expandedRows)
        : new Set(expandSingleRow && rows.length === 1 ? [0] : [])
      expandedRowHeights = saved ? new Map(saved.expandedRowHeights) : new Map()
      fkSubview = saved?.fkSubview ?? null
      newRowDrafts = saved?.newRowDrafts?.length ? saved.newRowDrafts.map((d) => ({ ...d })) : null
      newRowFocusCol = saved?.newRowFocusCol ?? null
      newRowFocusIdx = 0
      const restored = loadPendingChanges(newKey)
      pendingEdits = restored.edits
      pendingDeletes = restored.deletes
      // Always reset non-content states
      focusedRow = null
      focusedCol = null
      selAnchor = null
      pastEdits = []
      futureEdits = []
      selectedCols = new Set()
      _lastHeaderClickedCol = null
      _lastTabKey = newKey
    })
  });

  /**
   * Is this keystroke aimed at something you type into?
   *
   * The grid's chords are bound on the scroll container, so everything typed
   * into anything inside it - the staged-row band, the inline cell editor, a
   * picker's search box - reaches the grid's handlers too. ⌘A was the one that
   * showed: it selected every row in the table while the caret sat in a draft
   * field, where it means "select this value". By element rather than by state,
   * so a field added later is covered without anyone remembering to come back
   * here, and `[data-new-row]` covers the band's non-input controls as well.
   * @param {EventTarget | null} t
   */
  function isFieldTarget(t) {
    return (
      t instanceof HTMLElement &&
      (!!t.closest('[data-new-row]') ||
        t.isContentEditable ||
        t instanceof HTMLInputElement ||
        t instanceof HTMLTextAreaElement ||
        t instanceof HTMLSelectElement)
    )
  }

  // Document-level capture so undo/redo fires even during the brief window between
  // editingCell being cleared and the container div regaining focus.
  $effect(() => {
    function onCapture(/** @type {KeyboardEvent} */ e) {
      if (!isTableFocused || editingCell) return;
      // This listener is on `window`, so it runs before every other capture
      // handler in the app - including the one that gives inputs their undo
      // stack and their word deletion. Undoing a cell edit because the caret
      // was in a staged row and you pressed ⌘Z is the grid reaching into a
      // field it does not own, and it took the field's own ⌘Z with it.
      if (isFieldTarget(e.target)) return;
      if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "z" || e.key === "Z")) {
        e.preventDefault();
        e.shiftKey ? void redoEdit() : void undoEdit();
      } else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.key === "y" || e.key === "Y")) {
        e.preventDefault();
        void redoEdit();
      } else if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.key === "c" || e.key === "C")) {
        // Copy the block selection as TSV. Handled at document capture so it fires
        // regardless of which element inside the grid holds focus.
        if (computeCellRange()) { e.preventDefault(); void copyCellRange(); }
        else if (selectedCols.size) { e.preventDefault(); void copyColSelection(); }
        else if (focusedRow !== null && focusedCol !== null) {
          const ai = visToActualColIdx(focusedCol);
          if (ai >= 0) { e.preventDefault(); void copyCellValue(focusedRow, ai); }
        }
      }
    }
    window.addEventListener("keydown", onCapture, true);
    return () => window.removeEventListener("keydown", onCapture, true);
  });

  // Scroll is wired via the template `onscroll` handler (reactive) → onContainerScroll.
  // This effect only tracks the viewport size.
  $effect(() => {
    const container = tableContainer
    if (!container) return
    // The container is the only dependency. `physToVirt` reads `_physScrollTop`
    // and `_scrollScale`; tracked, they re-ran this effect on every scroll frame.
    // Each run re-measured with the integer `clientWidth`, then the observer it
    // re-created answered with the fractional contentRect width, so
    // `_viewportWidth` flipped twice a frame. Every flip re-ran the canvas layout
    // effect (which cancels the scroll blit) and re-laid-out the expanded-row
    // panels at a new width - a scroll with a JSON panel open ran at ~20fps.
    untrack(() => {
      _viewportWidth = container.clientWidth
      _viewportHeight = container.clientHeight
      _physScrollTop = Math.round(container.scrollTop)
      _scrollTop = physToVirt(_physScrollTop)
      _scrollLeft = Math.round(container.scrollLeft)
    })

    // Use contentRect directly - it's provided synchronously by the ResizeObserver
    // entry with no forced layout reflow. Removing the rAF here eliminates one full
    // frame of latency between the resize and the canvas redraw.
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect
      if (!r) return
      invalidateCanvasRect() // size/layout change can move the canvas - refresh hit-test rect
      _viewportWidth = r.width
      _viewportHeight = r.height
    })
    ro.observe(container)
    return () => ro.disconnect()
  })

  // Continuous rAF loop that drives canvas redraws during scroll.
  // Reading scrollTop/scrollLeft inside rAF gives the compositor-synchronized
  // position for the frame being painted, eliminating the 1-frame lag that
  // a one-shot scheduleDraw() produces (queued from onscroll → fires next frame).
  // The loop runs while the user is scrolling and for 200ms after the last
  // scroll event to cover momentum/inertia, then stops to save GPU time.
  let _scrollLoopId = 0
  let _scrollLoopDeadline = 0
  // Last position the loop actually painted - lets it skip identical frames during
  // the momentum tail / step scrolling instead of re-running a full redraw for a
  // frame where nothing moved. Content changes (hover/edits) arrive through
  // scheduleDraw(), which folds into this loop via _loopNeedsDraw while it runs,
  // so skipping unchanged-position frames never drops a requested repaint.
  let _loopLastTop = -1
  let _loopLastLeft = -1
  // Repaint requested (via scheduleDraw) while the scroll loop owns the frame -
  // the loop draws it instead of a second rAF double-painting the same frame.
  let _loopNeedsDraw = false
  // Vertical content delta, in CSS px, for a frame that differs from the last by
  // NOTHING BUT a vertical scroll - draw() may then blit the unchanged band
  // instead of repainting it (see the blit block there). Only the scroll loop
  // ever sets this, and draw() zeroes it on entry, so every other path into
  // draw() is a full repaint by construction.
  let _blitDy = 0

  function startScrollLoop() {
    _scrollLoopDeadline = performance.now() + 200
    if (_scrollLoopId) return
    _loopLastTop = -1
    _loopLastLeft = -1
    _isScrolling = true
    function loop() {
      const el = tableContainer
      if (!el || !_ctx || _fatalError || performance.now() > _scrollLoopDeadline) {
        _scrollLoopId = 0
        _isScrolling = false
        _loopNeedsDraw = false
        // Always repaint in full once scrolling stops, even if nothing asked for
        // it. The blit path below reuses pixels from the previous frame, so this
        // is what guarantees the view the user is left looking at was rendered
        // from scratch - any artifact a blit could produce lives only while the
        // content is still moving, and never in a resting grid.
        _blitDy = 0
        scheduleDraw()
        return
      }
      // Snap to whole CSS pixels. The canvas is sticky-pinned at the viewport's
      // integer left edge, so drawing content at a fractional scrollLeft puts text
      // and gridlines on sub-pixel x - WebKit then re-antialiases them every frame,
      // which reads as horizontal "vibration". Integer offsets render stably.
      const st = Math.round(el.scrollTop)
      const sl = Math.round(el.scrollLeft)
      if (st !== _loopLastTop || sl !== _loopLastLeft || _loopNeedsDraw) {
        const prevVirt = _scrollTop
        _physScrollTop = st
        _scrollTop = physToVirt(st)
        // Offer draw() a blit only when this frame is a pure vertical move of the
        // last one: same horizontal offset, a previous frame to copy from, and no
        // repaint pending (scheduleDraw sets _loopNeedsDraw for every content,
        // hover, focus, selection, theme and geometry change, so that flag is the
        // single "something other than position changed" signal). draw() applies
        // the remaining, geometric preconditions.
        _blitDy =
          sl === _loopLastLeft && _loopLastTop >= 0 && !_loopNeedsDraw
            ? _scrollTop - prevVirt
            : 0
        _scrollLeft = sl
        _loopLastTop = st
        _loopLastLeft = sl
        _loopNeedsDraw = false
        try {
          draw()
        } catch (err) {
          reportFatal(err)
          _scrollLoopId = 0
          _isScrolling = false
          return
        }
      }
      _scrollLoopId = requestAnimationFrame(loop)
    }
    _scrollLoopId = requestAnimationFrame(loop)
  }

  /** @param {Event & { currentTarget: HTMLElement }} e */
  function onContainerScroll(e) {
    const el = e.currentTarget
    invalidateCanvasRect()
    // Update state immediately so any synchronous consumers (hit-test etc.) are current.
    // Rounded to whole pixels to avoid sub-pixel shimmer during horizontal scroll.
    _physScrollTop = Math.round(el.scrollTop)
    _scrollTop = physToVirt(_physScrollTop)
    _scrollLeft = Math.round(el.scrollLeft)
    // Cancel any pending one-shot draw - the loop handles all scroll redraws at
    // the display's native frame rate (120Hz on ProMotion).
    if (_drawRafId) { cancelAnimationFrame(_drawRafId); _drawRafId = 0 }
    startScrollLoop()
    // Infinite scroll - trigger load when within 3 rows of the bottom
    if (infiniteScroll && !loadingMore) {
      const threshold = ROW_HEIGHT * 3
      if (el.scrollTop + el.clientHeight >= el.scrollHeight - threshold) {
        onloadmore()
      }
    }
  }

  // ── Canvas backing context + colour reader ────────────────────────────────
  // Plain (non-reactive) holders. Canvas sizing + drawing happen together in the
  // single master effect below - keeping them in ONE effect avoids any read+write
  // ping-pong between separate effects.
  /** @type {CanvasRenderingContext2D | null} */
  let _ctx = null
  // Reused scratch buffer for per-frame vertical grid separators (see draw()) - a
  // module-lifetime array so the scroll hot path does zero allocation for it.
  const _vSepsBuf = /** @type {number[]} */ ([])
  // Memoized rectangular-range column-name set for draw() - rebuilt only when the
  // range bounds or the navigable-column set change, not every frame during an
  // active range-select. The Set is read-only downstream (.has / iteration).
  let _rangeColNamesCache = /** @type {Set<string> | null} */ (null)
  let _rangeColNamesC0 = -1
  let _rangeColNamesC1 = -1
  let _rangeColNamesCols = /** @type {unknown} */ (null)
  /** @type {ReturnType<typeof createColorReader> | null} */
  let _readColor = null
  /** Canvas font strings measured from the DOM so they exactly match the app's
   *  computed type scale + the real loaded mono font (avoids fallback tofu). */
  let _fonts = /** @type {{ cell: string, type: string, header: string, family: string, cellPx: number, typePx: number } | null} */ (null)

  /**
   * Canvas text sizes, in logical px at 100% zoom.
   *
   * The grid sizes its own text instead of reading `--fs-xs` / `--fs-3xs` off
   * the probe, because those two numbers are produced by different formulas and
   * drift apart as you zoom: every `--fs-*` step is rounded on its own and then
   * nudged by the type scale's separation walk, while ROW_HEIGHT and every other
   * canvas constant is `round(N * canvasZoom)`. The result was text that changed
   * size relative to its own row from one rung to the next - 12px in a 28px row
   * at 100%, but proportionally 6% smaller at 110% - which is what made zooming
   * the grid look like the font was drifting rather than scaling.
   *
   * Scaling both from `canvasZoom` locks the ratio. The values are the nominal
   * `text-ui-xs` / `text-ui-3xs` sizes from DESIGN_SYSTEM.md, which is what the
   * canvas constants (ROW_HEIGHT, CELL_PAD_X, HEADER_H) were tuned against.
   */
  // The grid's two type sizes, at 100% zoom, in px.
  //
  // These are `text-ui-2xs` and `text-ui-3xs` resolved at the 16px root - the
  // cell size was 12 and 10, a step below the scale, which is what made the grid
  // read small against a sidebar rendering its rows at 15px.
  //
  // Quoted as plain numbers and multiplied by `canvasZoom` ON PURPOSE, rather
  // than read off a `.text-ui-*` probe. Both the CSS scale and `canvasZoom` are
  // pure functions of the same zoom rung, so the probe would only restate what
  // `canvasZoom` already knows - but it would restate it from a second source,
  // read at a different moment. That is exactly how the font and the geometry
  // came apart: `_fonts` is a cache, the zoom watcher clears it, and whichever
  // draw ran before `applySettings()` had written the new `--fs-*` vars to the
  // root cached the OUTGOING rung's size. Reset from 200% and you got 25px text
  // in a 30px row, permanently, because nothing invalidated the cache again.
  //
  // One source cannot disagree with itself. `13 * zoom` also tracks the real
  // scale to within a pixel at every rung (see type-scale.js), so the fidelity
  // the probe would have bought is a rounding step at 150% and above. */
  // Cell size is the user's (Settings → Appearance → Grid text size); the type
  // annotation rides 2px below it so the pair keeps its relationship at any size.
  const GRID_CELL_PX = $derived($appGridFontSize)
  const GRID_TYPE_PX = $derived(Math.max(7, $appGridFontSize - 2))

  /** Read the real computed mono FAMILY off the probe; sizes come from the zoom. */
  function readFonts(/** @type {HTMLElement} */ probe) {
    const prevClass = probe.className
    // The probe supplies the FAMILY only - that one really does have to come off
    // the DOM, because `--font-mono` is a user setting with no numeric form.
    probe.className = 'font-mono text-ui-xs'
    const family = getComputedStyle(probe).fontFamily
    probe.className = prevClass
    // Floors keep the smallest rung legible rather than sub-pixel mush.
    const cellPx = Math.max(9, Math.round(GRID_CELL_PX * canvasZoom))
    const typePx = Math.max(7, Math.round(GRID_TYPE_PX * canvasZoom))
    return {
      family,
      cellPx,
      typePx,
      cell: `${cellPx}px ${family}`,
      type: `${typePx}px ${family}`,
      // Medium-weight header name (Linear/Drizzle style).
      header: `530 ${cellPx}px ${family}`,
    }
  }

  // Repaint once webfonts finish loading - the canvas may first paint with a
  // fallback font that lacks glyphs (e.g. ʻ, macrons) and renders them as tofu.
  $effect(() => {
    if (typeof document === 'undefined' || !document.fonts) return
    let cancelled = false
    document.fonts.ready.then(() => {
      if (cancelled) return
      _fonts = null
      _readColor = null
      _redrawToken++
    })
    return () => { cancelled = true }
  })

  // Recreate the colour cache when the theme flips (the probe resolves the new
  // computed colours). The bump runs in a MutationObserver callback (not an
  // effect body), so reading _redrawToken here is not a tracked dependency.
  $effect(() => {
    const probe = colorProbe
    if (!probe || typeof MutationObserver === 'undefined') return
    const mo = new MutationObserver(() => {
      _readColor = createColorReader(probe)
      _fonts = null // font family may have changed (--font-mono) - re-measure
      _redrawToken++
    })
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'style', 'data-theme'] })
    return () => mo.disconnect()
  })

  // Dedicated zoom watcher - runs the moment zoomState.value changes in any tab,
  // clears the font cache so they're re-measured at the new size, and bumps
  // _redrawToken to guarantee a full canvas repaint immediately.
  $effect(() => {
    const z = zoomState.value  // subscribe to the store directly
    untrack(() => {
      _fonts = null            // discard cached font metrics - zoom may change them
      _redrawToken++
    })
  })

  // Same for the grid text size: `_fonts` holds the measured px, so changing the
  // setting without dropping the cache would move nothing until the next theme
  // flip or webfont load.
  $effect(() => {
    void $appGridFontSize
    untrack(() => { _fonts = null; _redrawToken++ })
  })

  // The focused-cell highlight is painted directly on the canvas by draw(),
  // which depends on focusedRow/focusedCol and so repaints on focus changes.

  // ── Experimental Vim mode (grid normal-mode navigation) ─────────────────────
  // Active only while $appVimMode is on and no cell is being edited. Reflects the
  // grid's mode into the shared status-bar indicator.
  let _vimPrefix = ''            // '', 'g', 'd', 'y' - awaiting the second key
  let _vimPrefixTimer = /** @type {ReturnType<typeof setTimeout> | 0} */ (0)
  let _vimCount = ''             // numeric motion prefix, e.g. "3" in 3j
  /** @param {string} p */
  function _setVimPrefix(p) {
    _vimPrefix = p
    if (_vimPrefixTimer) clearTimeout(_vimPrefixTimer)
    if (p) _vimPrefixTimer = setTimeout(() => { _vimPrefix = '' }, 700)
  }
  /** Move the cell cursor by (dr, dc), clamped, and keep it in view. */
  function vimMove(dr, dc) {
    const rowLen = rows.length, visLen = navigableColumns.length
    if (!rowLen || !visLen) return
    focusedRow = Math.min(Math.max((focusedRow ?? 0) + dr, 0), rowLen - 1)
    focusedCol = Math.min(Math.max((focusedCol ?? 0) + dc, 0), visLen - 1)
    selAnchor = null
    scrollRowIntoView(focusedRow)
    scheduleDraw()
  }
  /** Interpret a key as a grid Vim normal-mode command. Returns true if consumed. */
  function handleVimGridKey(e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return false
    const k = e.key
    if (k === 'Escape') {
      _setVimPrefix(''); _vimCount = ''
      return false // let the default Escape cascade (clear range / blur) run
    }
    // Only single printable characters are Vim commands; arrows, Tab, Enter,
    // Home/End, Page… keep their normal grid behavior in both modes.
    if (k.length !== 1) return false
    e.preventDefault()

    // Resolve a pending g/d/y prefix first.
    if (_vimPrefix) {
      const p = _vimPrefix; _setVimPrefix('')
      if (p === 'g' && k === 'g') { focusedRow = 0; scrollRowIntoView(0); scheduleDraw(); return true }
      if (p === 'd' && k === 'd') { deleteRow(focusedRow ?? 0); return true }
      if (p === 'y' && k === 'y') { void copyAs(focusedRow ?? 0, 'plain'); return true }
      // non-matching second key falls through to be handled fresh below
    }

    // Numeric count prefix (a lone "0" is the row-start motion, not a count).
    if ((k >= '1' && k <= '9') || (k === '0' && _vimCount !== '')) { _vimCount += k; return true }
    const count = Math.max(1, parseInt(_vimCount || '1', 10)); _vimCount = ''
    const ai = () => visToActualColIdx(focusedCol ?? 0)

    switch (k) {
      case 'g': case 'd': case 'y': _setVimPrefix(k); return true
      case 'h': vimMove(0, -count); return true
      case 'l': vimMove(0, count); return true
      case 'j': vimMove(count, 0); return true
      case 'k': vimMove(-count, 0); return true
      case '0': focusedCol = 0; scheduleDraw(); return true
      case '$': focusedCol = navigableColumns.length - 1; scheduleDraw(); return true
      case 'G': { const last = rows.length - 1; if (last >= 0) { focusedRow = last; scrollRowIntoView(last); scheduleDraw() } return true }
      case 'i': case 'a': case 'c': startEdit(focusedRow ?? 0, ai()); setVimSubMode('insert'); return true
      case 'x': setCellNull(focusedRow ?? 0, ai()); return true
      case '/': onrequestsearch(); return true
      default: return true // unmapped printable char - consume so it can't type-to-edit
    }
  }
  // Mirror the grid's edit state into the shared Vim indicator while focused.
  $effect(() => {
    if (!$appVimMode || !isTableFocused) return
    setVimSubMode(editingCell ? 'insert' : 'normal')
  })

  /** @param {KeyboardEvent} e */
  function handleTableKeydown(e) {
    // A keystroke aimed at a field is the field's. Checked first, because the
    // chords sit above this in the function.
    if (isFieldTarget(e.target)) return
    // Every move from here is a keyboard move, so the cursor may scroll itself
    // into view. Set before the branches rather than in each of them.
    _focusFromKey = true
    // Ctrl/Cmd + / - / 0: zoom the whole app (canvas scales in lockstep).
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey) {
      if (e.key === '=' || e.key === '+') { e.preventDefault(); increaseZoom(); return }
      if (e.key === '-')                  { e.preventDefault(); decreaseZoom(); return }
      if (e.key === '0')                  { e.preventDefault(); resetZoom(); return }
    }

    // Ctrl+A: select all rows - but while editing a cell, let the input handle
    // its native "select all text" (don't preventDefault, or it's swallowed).
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "a" || e.key === "A")) {
      if (!editingCell) {
        e.preventDefault();
        // ⌘⇧A deselects, the way it does in every editor and image tool that
        // has a select-all. Escape also clears a selection, but it is the key
        // that unwinds everything else on screen too - the sub-view, a range, a
        // column selection - so it is never the one you reach for when the only
        // thing you want back is an empty checkbox column.
        if (e.shiftKey) {
          selected = new Set();
          selectedCols = new Set();
          _lastHeaderClickedCol = null;
          scheduleDraw();
        } else {
          selected = allRowIndexSet();
        }
      }
      return;
    }

    // Space: the focused cell, full size. Space is a printable character, so
    // without this it fell through to type-to-edit below and opened the editor
    // with a space typed into it - the one keystroke on the grid that destroyed
    // the cell it was aimed at. Enter and any other character still start an
    // edit; Space previews. Shift+Space stays bound to the same thing, which is
    // what it was before.
    // Alt+Space steps into the dock the Space beside it opened. Space leaves the
    // cursor on the grid on purpose, so arrows keep walking the table and the
    // preview follows; this is the deliberate way in. Escape brings focus back.
    if (e.key === " " && e.altKey && !e.ctrlKey && !e.metaKey) {
      if (cellEditorOpen && cellEditorRef?.focusEditor()) {
        e.preventDefault();
        return;
      }
    }

    // Alt+J: the whole row as JSON in the dock. Alt+Space steps into it, the
    // same as it does for a cell, and Escape closes it.
    if (e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "j" || e.key === "J")) {
      if (!editingCell && focusedRow !== null) {
        e.preventDefault();
        openRowJson(focusedRow);
        return;
      }
    }

    // Space previews the focused cell, full size. It is a printable character,
    // so without this it fell through to type-to-edit below and opened the
    // editor with a space typed into it - the one keystroke on the grid that
    // destroyed the cell it was aimed at.
    //
    // Plain Space leaves the cursor on the grid, so arrows keep walking the
    // table and the dock follows along. Shift+Space is the same preview and
    // steps into the editor too: caret at the end of a short value, which is
    // one you mean to edit, and at the top of a long one, which is one you mean
    // to read.
    if (e.key === " " && !e.ctrlKey && !e.metaKey && !e.altKey) {
      if (!editingCell && focusedRow !== null && focusedCol !== null) {
        const ai = visToActualColIdx(focusedCol);
        if (ai >= 0) {
          e.preventDefault();
          // Passed into the open, not chased afterwards: the editor inside the
          // dock is lazy-loaded, so a focus call made from out here on the tick
          // after opening can land before it exists, or before the seed
          // replaces its document. The panel already knows how to wait for its
          // own editor, so this just tells it to.
          openCellEditor(focusedRow, ai, e.shiftKey);
          return;
        }
      }
    }

    // Ctrl+C (copy selection/range/cell) is handled by the document-capture
    // listener above so it works regardless of which grid element holds focus.

    // Undo / redo - active even while the cell input has focus
    if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "z" || e.key === "Z")) {
      if (!editingCell) {
        e.preventDefault();
        e.shiftKey ? void redoEdit() : void undoEdit();
        return;
      }
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.key === "y" || e.key === "Y")) {
      if (!editingCell) { e.preventDefault(); void redoEdit(); return; }
    }
    // ⌘/Ctrl+Enter and Shift+Enter, when not editing.
    //
    // On a cell that holds a foreign key they follow it: ⌘↵ navigates to the
    // referenced row (which is what the shortcuts dialog and the cell menu have
    // always said it does - the chord used to start an edit instead, so the one
    // documented FK keystroke was the one that did not work), and ⇧↵ opens that
    // target in a NEW tab, beside the row you are reading. On any other cell
    // both start the edit ⌘↵ started before.
    if (!editingCell && !e.altKey && e.key === "Enter" && ((e.ctrlKey || e.metaKey) || e.shiftKey)) {
      e.preventDefault();
      if (focusedRow !== null && focusedCol !== null) {
        // A relationship column reads the chords the same way a foreign key
        // does: ⇧↵ opens the related rows in a new tab, ⌘↵ opens them in place.
        // Plain Enter keeps the dock preview.
        const vrel = vrelAtVisIdx(focusedCol);
        if (vrel) {
          openReverseFkFullView(focusedRow, vrel, { newTab: e.shiftKey && !(e.ctrlKey || e.metaKey) });
          return;
        }
        const ai = visToActualColIdx(focusedCol);
        if (ai >= 0) {
          // Shift alone means the new tab; ⌘⇧↵ follows in place, like ⌘↵.
          const newTab = e.shiftKey && !(e.ctrlKey || e.metaKey);
          if (tryFollowForeignKey(focusedRow, ai, e, { newTab })) return;
          startEdit(focusedRow, ai);
        }
      } else { focusedRow = 0; focusedCol = 0; }
      return;
    }


    if (editingCell) return;

    // Mod+Escape clears the whole band. Escape inside a row drops that row, and
    // with eight staged that is eight presses; this is the way out of all of it.
    if (e.key === 'Escape' && (e.ctrlKey || e.metaKey) && newRowDrafts?.length) {
      e.preventDefault();
      cancelNewRow();
      return;
    }

    // The cell menu's two quick filters, as chords. Alt+F sits beside
    // Alt+A, which opens the filter menu: same family, one step shorter,
    // and Alt+E is the other half of the pair. Handled before the switch so a
    // plain `f` or `e` still reaches type-to-edit.
    if (!editingCell && e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "f" || e.key === "F" || e.key === "e" || e.key === "E")) {
      if (focusedRow !== null && focusedCol !== null) {
        const ai = visToActualColIdx(focusedCol);
        const col = columns[ai];
        if (ai >= 0 && col) {
          e.preventDefault();
          onfilterbyvalue(col.name, rows[focusedRow]?.[ai], e.key === "e" || e.key === "E");
          return;
        }
      }
    }

    // Alt+D copies the focused row into the insert draft, beside the Alt+F /
    // Alt+E pair - all three act on what the cursor is standing on.
    if (!editingCell && e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "d" || e.key === "D")) {
      if (!readonly && focusedRow !== null) {
        e.preventDefault();
        duplicateRow(focusedRow);
        return;
      }
    }

    // Mod+E toggles the focused row's detail panel - the same thing the gutter
    // chevron and the context menu's Expand do, for a hand already on the
    // keyboard. Below the `editingCell` guard on purpose: inside the inline
    // editor the chord belongs to the text field.
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && (e.key === "e" || e.key === "E")) {
      if (showRowExpand && focusedRow !== null) { e.preventDefault(); toggleRowExpand(focusedRow); }
      return;
    }

    // Experimental Vim normal-mode: intercept plain command keys (hjkl, gg/G,
    // i/x/dd/yy, …) before the default arrow / type-to-edit handling.
    if ($appVimMode && handleVimGridKey(e)) return;

    // Modified arrows belong to the app-level handler in StudioShell, not to the
    // cell cursor: Cmd/Ctrl+Arrow is table-level navigation (scroll to top/bottom,
    // first/last column, paginate) and Alt+Arrow is Go Back / Go Forward. Let them
    // bubble untouched - the old double-handling (cursor jumped one cell AND the
    // view navigated) is what made both shortcuts feel broken.
    if (
      (e.ctrlKey || e.metaKey || e.altKey) &&
      (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "ArrowRight" || e.key === "ArrowLeft")
    ) {
      return;
    }

    const visLen = navigableColumns.length;
    const rowLen = rows.length;
    if (!rowLen || !visLen) return;

    const curRow = focusedRow ?? 0;
    const curCol = focusedCol ?? 0;

    // Range selection is disabled - a plain arrow / Tab just collapses any stray
    // range back to the single focused cell. (Shift+Arrow no longer extends a
    // rectangular range; nothing sets selAnchor - see its declaration.)
    if (
      e.key === "ArrowDown" || e.key === "ArrowUp" ||
      e.key === "ArrowRight" || e.key === "ArrowLeft" || e.key === "Tab"
    ) {
      clearCellRange();
    }

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        const nr = Math.min(curRow + 1, rowLen - 1);
        focusedRow = nr; if (focusedCol === null) focusedCol = 0;
        scrollRowIntoView(nr);
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        const pr = Math.max(curRow - 1, 0);
        focusedRow = pr; if (focusedCol === null) focusedCol = 0;
        scrollRowIntoView(pr);
        break;
      }
      case "ArrowRight": {
        e.preventDefault();
        if (focusedRow === null) { focusedRow = 0; focusedCol = 0; break; }
        if (curCol < visLen - 1) { focusedCol = curCol + 1; }
        else if (curRow < rowLen - 1) { focusedRow = curRow + 1; focusedCol = 0; scrollRowIntoView(curRow + 1); }
        break;
      }
      case "ArrowLeft": {
        e.preventDefault();
        if (focusedRow === null) { focusedRow = 0; focusedCol = 0; break; }
        if (curCol > 0) { focusedCol = curCol - 1; }
        else if (curRow > 0) { focusedRow = curRow - 1; focusedCol = visLen - 1; scrollRowIntoView(curRow - 1); }
        break;
      }
      case "Tab": {
        e.preventDefault();
        if (e.shiftKey) {
          if (focusedRow === null) { focusedRow = 0; focusedCol = 0; break; }
          if (curCol > 0) { focusedCol = curCol - 1; }
          else if (curRow > 0) { focusedRow = curRow - 1; focusedCol = visLen - 1; scrollRowIntoView(curRow - 1); }
        } else {
          if (focusedRow === null) { focusedRow = 0; focusedCol = 0; break; }
          if (curCol < visLen - 1) { focusedCol = curCol + 1; }
          else if (curRow < rowLen - 1) { focusedRow = curRow + 1; focusedCol = 0; scrollRowIntoView(curRow + 1); }
        }
        break;
      }
      case "Enter":
      case "F2": {
        e.preventDefault();
        if (focusedRow !== null && focusedCol !== null) {
          // A relationship column has nothing to edit - Enter opens it, which is
          // the only thing the cell does.
          // Plain Enter previews the relation in the dock - a click's half of the
          // pair. The modified chords are handled above, before this switch.
          const vrel = vrelAtVisIdx(focusedCol);
          if (vrel) { toggleReverseFkSubview(focusedRow, vrel); break; }
          const ai = visToActualColIdx(focusedCol);
          if (ai >= 0) startEdit(focusedRow, ai);
        } else { focusedRow = 0; focusedCol = 0; }
        break;
      }
      case "Escape": {
        e.preventDefault();
        // Priority: close FK sub-view → collapse cell range → clear col selection
        // → close the most-recently-expanded row → clear cell focus.
        if (cellEditorOpen) { cellEditorOpen = false; break; }
        if (fkSubview !== null) { fkSubview = null; break; }
        if (computeCellRange()) { clearCellRange(); scheduleDraw(); break; }
        if (selectedCols.size) { selectedCols = new Set(); _lastHeaderClickedCol = null; scheduleDraw(); break; }
        if (expandedRows.size > 0) { toggleRowExpand(/** @type {number} */ ([...expandedRows].pop())); break; }
        focusedRow = null; focusedCol = null;
        break;
      }
      case "Delete": {
        // A modified Delete belongs to someone else: ⌘/Ctrl+Delete deletes the
        // row and Alt+Delete discards staged changes, both at the app level.
        // Clearing this cell is what the UNMODIFIED key means.
        if (e.ctrlKey || e.metaKey || e.altKey) break;
        if (focusedRow !== null && focusedCol !== null) {
          const ai = visToActualColIdx(focusedCol);
          if (ai >= 0 && canEditColumn(ai)) { e.preventDefault(); void setCellNull(focusedRow, ai); }
        }
        break;
      }
      case "Backspace": {
        // Same for a modified ⌫: ⌘/Ctrl+⌫ deletes the row, Alt+⌫ discards every
        // staged change. Starting an edit here is what swallowed both - focus
        // moved into an input, and the app-level handler then bowed out of it
        // the way it bows out of any text field.
        if (e.ctrlKey || e.metaKey || e.altKey) break;
        if (focusedRow !== null && focusedCol !== null) {
          const ai = visToActualColIdx(focusedCol);
          if (ai >= 0 && canEditColumn(ai)) {
            const col = columns[ai];
            const isSelectOrToggle = !!getColumnEnumValues(col) || isBooleanType(col?.dataType ?? col?.data_type ?? "");
            if (!isSelectOrToggle) { e.preventDefault(); startEdit(focusedRow, ai, ""); }
          }
        }
        break;
      }
      case "Home": {
        e.preventDefault();
        if (e.ctrlKey || e.metaKey) { focusedRow = 0; focusedCol = 0; scrollRowIntoView(0); }
        else { focusedCol = 0; }
        break;
      }
      case "End": {
        e.preventDefault();
        if (e.ctrlKey || e.metaKey) { focusedRow = rowLen - 1; focusedCol = visLen - 1; scrollRowIntoView(rowLen - 1); }
        else { focusedCol = visLen - 1; }
        break;
      }
      case "PageDown": {
        e.preventDefault();
        const pdn = Math.min(curRow + 10, rowLen - 1);
        focusedRow = pdn; if (focusedCol === null) focusedCol = 0;
        scrollRowIntoView(pdn);
        break;
      }
      case "PageUp": {
        e.preventDefault();
        const pup = Math.max(curRow - 10, 0);
        focusedRow = pup; if (focusedCol === null) focusedCol = 0;
        scrollRowIntoView(pup);
        break;
      }
      default: {
        // Printable character → start editing with that char (type-to-edit)
        if (
          e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey &&
          focusedRow !== null && focusedCol !== null
        ) {
          const ai = visToActualColIdx(focusedCol);
          if (ai >= 0 && canEditColumn(ai)) {
            const col = columns[ai];
            const isSelectOrToggle = !!getColumnEnumValues(col) || isBooleanType(col?.dataType ?? col?.data_type ?? "");
            if (!isSelectOrToggle) { e.preventDefault(); startEdit(focusedRow, ai, e.key); }
          }
        }
        break;
      }
    }
  }

  // ── Canvas drawing ─────────────────────────────────────────────────────────
  const CELL_PAD_X = $derived(Math.round(10 * canvasZoom))
  const ICON_HIT = $derived(Math.round(24 * canvasZoom))

  // The geometry, type sizes and appearance flags the canvas draws with, handed
  // to the related-rows panel so its DOM table reads as one surface with the
  // grid above it: same row height, same text size, same rules, same zebra, same
  // alignment. `_fonts` itself is a nulled-and-refilled draw cache, not
  // reactive, so the panel gets the derived numbers it is built from instead.
  const gridMetrics = $derived({
    zoom: canvasZoom,
    cellPx: Math.max(9, Math.round(GRID_CELL_PX * canvasZoom)),
    typePx: Math.max(7, Math.round(GRID_TYPE_PX * canvasZoom)),
    rowH: ROW_HEIGHT,
    headerH: HEADER_H,
    padX: CELL_PAD_X,
    rowRules: _tableStyle.rows === true,
    colRules: _tableStyle.cols === true,
    zebra: _tableStyle.zebra === true,
    // The rest of the chosen style, for surfaces that draw their own table and
    // have to land on the same look: dashes, weight, and the heavier rule every
    // Nth row. Three booleans were not enough - every dashed, dotted, ledger or
    // bordered preset came out as plain solid lines in the FK sub-view.
    dash: _tableStyle.dash ?? null,
    double: _tableStyle.double === true,
    strong: _tableStyle.strong === true,
    groupEvery: _tableStyle.groupEvery ?? 0,
    align: $appTableAlign,
    rowNumbers: $appRowNumbers === true,
  })

  // Cached glyph advance for the active ctx.font. Every table font is monospace,
  // so text width is O(1) (charCount × advance). This replaces the per-cell
  // measureText() binary search that was the main scroll-framerate bottleneck.
  // Re-measured automatically whenever ctx.font changes (zoom / cell vs header).
  let _glyphW = 0
  let _glyphFont = ''
  function _syncGlyphW(/** @type {CanvasRenderingContext2D} */ ctx) {
    if (ctx.font === _glyphFont) return
    const a = ctx.measureText('i').width
    const b = ctx.measureText('W').width
    _glyphW = Math.abs(a - b) < 0.1 ? b : 0 // 0 ⇒ not monospace → fall back to measureText
    _glyphFont = ctx.font
  }

  /** Width of `str` under the active font (O(1) for monospace). */
  function textWidth(/** @type {CanvasRenderingContext2D} */ ctx, /** @type {string} */ str) {
    _syncGlyphW(ctx)
    return _glyphW > 0 ? str.length * _glyphW : ctx.measureText(str).width
  }

  // Truncated strings, keyed by column width then by source string, for the
  // active font.
  //
  // Truncating is the most expensive thing in the draw loop: WebKit shapes the
  // whole string through HarfBuzz for every measureText, and a text-heavy table
  // measured ~16,000 characters per frame (113 calls, 3.1ms of an 11ms frame)
  // re-deriving answers it had already computed. The result is a pure function
  // of (font, width, string) and all three are constant while scrolling, so each
  // combination is measured once and the rest of the scroll is map hits.
  /** @type {Map<number, Map<string, string>>} */
  const _truncCache = new Map()
  /** The font `_truncCache` was built under; a zoom or font change drops it. */
  let _truncCacheFont = ''
  let _truncCacheEntries = 0
  const _TRUNC_CACHE_MAX = 8192

  function _truncCacheReset() {
    _truncCache.clear()
    _truncCacheEntries = 0
  }

  /** Truncate `text` to fit `maxW` px under the current ctx.font, adding `…`. */
  function truncText(ctx, text, maxW) {
    if (maxW <= 0) return ''
    const len = text.length
    if (len === 0) return text
    _syncGlyphW(ctx)
    // Fast no-truncation check: if the monospace estimate fits, trust it.
    if (_glyphW > 0 && len * _glyphW <= maxW) return text
    // Past here the string has to be measured, so consult the cache first.
    // `_glyphFont` was just refreshed from ctx.font by _syncGlyphW above.
    if (_truncCacheFont !== _glyphFont) { _truncCacheReset(); _truncCacheFont = _glyphFont }
    let byText = _truncCache.get(maxW)
    if (byText !== undefined) {
      const hit = byText.get(text)
      if (hit !== undefined) return hit
    }
    const out = _truncMeasure(ctx, text, maxW, len)
    if (_truncCacheEntries >= _TRUNC_CACHE_MAX) { _truncCacheReset(); byText = undefined }
    if (byText === undefined) { byText = new Map(); _truncCache.set(maxW, byText) }
    byText.set(text, out)
    _truncCacheEntries++
    return out
  }

  /** The measuring half of truncText - only reached on a cache miss. */
  function _truncMeasure(ctx, text, maxW, len) {
    // For truncation, always measure accurately - the monospace estimate can
    // over-count for non-ASCII characters (Arabic, CJK, etc.) that fall back
    // to a narrower font, leaving an apparent gap after the ellipsis.
    if (ctx.measureText(text).width <= maxW) return text
    // Monospace fast path: the slice length is plain arithmetic, verified with a
    // couple of real measurements (non-ASCII glyphs fall back to narrower fonts,
    // so a failed verify drops to the exact binary search below). This replaces
    // ~10 measureText binary-search probes per truncated cell per frame with 2-3.
    if (_glyphW > 0) {
      let k = Math.max(0, Math.min(len - 1, Math.floor(maxW / _glyphW) - 1))
      if (ctx.measureText(text.slice(0, k) + '…').width <= maxW) {
        // Grow greedily while it still fits (covers narrow fallback glyphs);
        // bounded so the worst case stays O(1).
        for (let step = 0; step < 4 && k < len - 1; step++) {
          if (ctx.measureText(text.slice(0, k + 1) + '…').width <= maxW) k++
          else break
        }
        return text.slice(0, k) + '…'
      }
    }
    let lo = 0, hi = len
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      if (ctx.measureText(text.slice(0, mid) + '…').width <= maxW) lo = mid
      else hi = mid - 1
    }
    return text.slice(0, lo) + '…'
  }

  /**
   * Hover-button rects for a cell (viewport coords).
   *
   * The buttons sit on whichever side the value is *not* using: right of
   * left-aligned text, left of right-aligned text. That way they always land in
   * the cell's empty space, so showing them neither covers the value nor pushes
   * it sideways. Reserving a fixed strip instead left a permanent dead gap down
   * the column.
   *
   * Must stay in step with the draw pass below - this is the click target for
   * what that paints.
   */
  /** True when a cell holds the "this is N bytes" stand-in rather than a value. */
  function isOversizeValue(v) {
    return !!v && typeof v === 'object' && /** @type {any} */ (v).__strokeOversize === true
  }

  /**
   * The per-cell Load control: a download arrow that becomes a spinner while the
   * value is on its way. Drawn rather than mounted - it lives in a canvas cell,
   * and the spinner's angle comes from the frame clock so it costs one arc.
   * @param {CanvasRenderingContext2D} ctx @param {number} x @param {number} cy
   * @param {boolean} busy @param {any} c
   */
  function drawCellLoad(ctx, x, cy, busy, c) {
    if (!busy) {
      drawIcon(ctx, 'download', x, cy - 7, 14, withAlpha(c.cPrimary, 0.9), 1.8)
      return
    }
    const r = 6
    ctx.save()
    ctx.strokeStyle = withAlpha(c.cPrimary, 0.9)
    ctx.lineWidth = 1.8
    ctx.lineCap = 'round'
    ctx.beginPath()
    ctx.arc(x + 7, cy, r, c.spinAngle, c.spinAngle + Math.PI * 1.35)
    ctx.stroke()
    ctx.restore()
  }

  function cellButtonRects(cellX, w, ry, rh, { alignRight = false, withLoad = false }) {
    const cy = ry + rh / 2
    const rect = (/** @type {number} */ x) => ({ x, y: ry, w: ICON_HIT, h: rh, cx: x + ICON_HIT / 2, cy })
    // The copy button sits on the side the value is not using; Load sits
    // immediately inboard of it, so the pair reads as one group wherever it is.
    if (alignRight) {
      const copy = rect(cellX + 4)
      return { copy, load: withLoad ? rect(cellX + 4 + ICON_HIT) : null }
    }
    const right = cellX + w - 4  // 4px right margin
    const copy = rect(right - ICON_HIT)
    return { copy, load: withLoad ? rect(right - ICON_HIT * 2) : null }
  }

  function draw() {
    const ctx = _ctx
    const read = _readColor
    if (!ctx || !read || !canvasEl || !colorProbe) return
    // Drop any scroll delta seen while holding: the first real frame after a
    // hold is a different table and must repaint whole, never blit the old one.
    if (holdPaint && _surfaceHasFrame) { _blitDy = 0; return }
    _surfaceHasFrame = true

    // Scratch surface for the scroll blit. One canvas, grown as needed and never
  // shrunk, so a scroll allocates nothing after its first frame.
  /** @type {{ canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D } | null} */
  let _blitScratch = null
  function blitScratch(/** @type {number} */ w, /** @type {number} */ h) {
    if (w <= 0 || h <= 0) return null
    if (!_blitScratch) {
      const canvas = document.createElement('canvas')
      const c = canvas.getContext('2d', { alpha: true })
      if (!c) return null
      _blitScratch = { canvas, ctx: c }
    }
    const { canvas, ctx: c } = _blitScratch
    if (canvas.width < w || canvas.height < h) {
      canvas.width = Math.max(canvas.width, w)
      canvas.height = Math.max(canvas.height, h)
    }
    return { canvas, ctx: c }
  }

  // ── Scroll blitting ──────────────────────────────────────────────────────
    // After the truncation cache, `fillText` is the whole remaining draw cost:
    // measured at 167 calls / 6,243 glyphs per frame, 4.8ms of an 8.7ms draw,
    // ~29us a call. That is Cairo rasterising glyphs on the CPU and no amount of
    // JS tuning reaches it. The only way past it is to stop redrawing text that
    // has not moved - on a frame that differs from the last one *only* by a
    // vertical scroll, the body is a rigid translation of what is already on the
    // canvas, so one drawImage moves the still-valid band and we repaint just the
    // strip the scroll uncovered. A one- or two-row step then paints one or two
    // rows instead of every visible one.
    //
    // The scroll loop opens the gate (pure vertical move, no repaint pending);
    // everything below is the geometric half of the precondition - the body must
    // be a rigid translation, which it is not when skeleton rows are animating,
    // when an insert draft adds a band above row 0, or when dy is fractional
    // (reachable only on a table large enough for the scroll range to be
    // compressed), which would land text on half-pixels and smear it.
    // `_sawSkeleton` is still the *previous* frame's value here; it is reset a few
    // lines down.
    //
    // An EXPANDED ROW used to disqualify a blit too, on the grounds that it makes
    // row heights non-uniform. It does, and it does not matter: a vertical scroll
    // is still a pure translation, and the panel's gap - which the canvas leaves
    // as bare background - translates with everything else. The strip loop places
    // rows by rowViewportY(), which already accounts for the expansion, and the
    // newly exposed strip is cleared to cPanel before anything is drawn into it,
    // so a gap landing inside it stays background. The one event that is NOT a
    // translation, the panel's measured height changing, is tracked by the
    // geometry effect below and forces a full frame through scheduleDraw().
    //
    // Keeping the guard cost a FULL repaint of every visible row on every frame
    // for as long as any row was expanded - doubled once the display was unclamped
    // to 120Hz. That saturates the main thread, and a saturated main thread is
    // what shows up as the grid banding dark and the toolbar and column header
    // stuttering: not a canvas bug, starvation.
    const dy = _blitDy
    _blitDy = 0
    const blitBodyH = Math.max(0, _viewportHeight - HEADER_H)
    // Backing-store pixels per CSS pixel. Both the header offset and the scroll
    // delta have to land on whole device pixels for the copy to be exact; at the
    // usual DPR of 1 or 2 they always do, and on a fractional DPR we simply fall
    // back to a full repaint for the frames where they do not.
    // The surface transform's own scale, so a `dy` in CSS px maps to whole
    // device px exactly as draw() places rows. Dividing the backing width by
    // the viewport width gave 1538 / 1537.5 on a fractional viewport, no `dy`
    // was ever an integer multiple of that, and the blit never ran.
    const blitScale = _surfaceDpr
    const canBlit =
      dy !== 0 &&
      Number.isInteger(dy) &&
      Math.abs(dy) < blitBodyH &&
      blitScale > 0 &&
      Number.isInteger(dy * blitScale) &&
      Number.isInteger(HEADER_H * blitScale) &&
      !_sawSkeleton &&
      !newRowDrafts &&
      visibleColumns.length > 0

    // Rebuilt as cells paint; pumpCellImages() below uses it to fetch only what
    // is actually on screen and to protect those thumbnails from eviction.
    // A blit frame paints only the exposed strip, so clearing here would drop
    // every thumbnail that merely survived the scroll; the set is instead left to
    // accumulate and is rebuilt by the next full frame, which the loop guarantees
    // arrives as soon as scrolling stops.
    if (!canBlit) _imgWanted.clear()
    // Font sizes and the layout constants (ROW_HEIGHT, HEADER_H, …) are both
    // `<base px> * canvasZoom`, so text and geometry cannot land on different
    // zoom rungs however this cache is invalidated. Do NOT re-source either one
    // from a `.text-ui-*` probe: the CSS scale says the same thing a moment
    // later, and the gap between the two is a bug, not a refinement.
    if (!_fonts) _fonts = readFonts(colorProbe)
    syncDisplayCaches()
    // Shimmer phase for skeleton rows, advanced from the clock so it moves at the
    // same rate whatever the frame rate. _sawSkeleton is reset here and set by any
    // skeleton row painted below; the tail uses it to decide whether to keep
    // animating, so a fully loaded grid schedules nothing at all.
    _sawSkeleton = false
    _skelMin = -1
    _skelMax = -1
    _shimmerFill = null
    if (_shimmerOn) _shimmerPhase = (performance.now() % SHIMMER_PERIOD) / SHIMMER_PERIOD

    const W = _viewportWidth
    const H = _viewportHeight
    const n = rows.length

    // Resolve theme colours (cached for the frame by the reader).
    const cPanel = read('var(--panel)')
    const cFg = read('var(--foreground)')
    const cMuted = read('var(--muted-foreground)')
    const cGrid = read('var(--table-grid)')
    const cBorder = read('var(--border)')
    const cRing = read('var(--ring)')
    const cMutedBg = read('var(--muted)')
    const cAccent = read('var(--accent)')
    const cPrimary = read('var(--primary)')
    // Primary cell value text - crisper than muted (Drizzle/Linear feel), but a
    // hair softer than full foreground so focused/dirty cells still stand out.
    const cText = withAlpha(cFg, 0.86)
    const AMBER = 'rgb(245, 158, 11)'
    const AMBER_FG = 'rgba(251, 191, 36, 0.85)'
    const BLUE_FG = 'rgba(96, 165, 250, 0.8)'
    // Staged-delete diff marker (red - matches the destructive accent).
    const RED = 'rgb(239, 68, 68)'

    ctx.imageSmoothingEnabled = false
    // Strip of body the scroll uncovered, in viewport px - the only part that
    // needs rows painted on a blit frame. Empty (stripH 0) on a full frame.
    let stripTop = 0
    let stripH = 0
    if (canBlit) {
      // The copy is done in backing-store pixels with the transform reset, source
      // and destination the same size, so it is always an exact 1:1 move of whole
      // device pixels. Going through the DPR-scaled transform instead would let a
      // fractional viewport size or a 1.5x DPR make source and destination differ
      // by a fraction of a pixel - drawImage would then *resample* the band, and
      // because each frame copies the previous one that blur would compound over a
      // scroll into visibly smeared text. `blitScale` is the scale
      // syncCanvasSurface applied, not raw devicePixelRatio, which it caps at 2.
      // Only the viewport band moves. The surface can be taller than the
      // viewport (syncCanvasSurface keeps the tallest height it has had) and
      // everything below the viewport on it is stale. Moving the whole surface
      // dragged that band into view one `dy` per frame once the viewport had
      // shrunk (a dock opened, the app zoomed in): rows from an old frame cut
      // across the live ones, and a blank band where the surface had been
      // cleared. Floored so a fractional height is repainted by the strip
      // rather than resampled by the copy.
      const devH = Math.floor(H * blitScale)
      const headDev = HEADER_H * blitScale
      const kDev = dy * blitScale // integer: canBlit required it
      // dy > 0 means the content moved up: keep the lower band, expose the bottom.
      const srcTop = headDev + (kDev > 0 ? kDev : 0)
      const dstTop = headDev + (kDev > 0 ? 0 : -kDev)
      const keepDev = devH - Math.max(srcTop, dstTop)
      // Through a scratch surface, never canvas-onto-itself.
      //
      // Source and destination overlap by everything but `dy` - that is the
      // point of the copy - and a self-drawImage across overlapping regions is
      // only safe if the engine snapshots the source first. WebKitGTK's canvas
      // is rasterised on the CPU by Cairo, which copies in place, so a band
      // could be read after it had already been written over: rows duplicated
      // above the seam and a torn frame that only ever appeared mid-scroll.
      //
      // Two copies instead of one, and both are flat surface moves with no text
      // in them - still nothing beside repainting 6,000 glyphs.
      const scratch = blitScratch(ctx.canvas.width, keepDev)
      if (scratch) {
        scratch.ctx.setTransform(1, 0, 0, 1, 0, 0)
        scratch.ctx.clearRect(0, 0, ctx.canvas.width, keepDev)
        scratch.ctx.drawImage(
          ctx.canvas,
          0, srcTop, ctx.canvas.width, keepDev,
          0, 0, ctx.canvas.width, keepDev,
        )
        ctx.save()
        ctx.setTransform(1, 0, 0, 1, 0, 0)
        ctx.drawImage(
          scratch.canvas,
          0, 0, ctx.canvas.width, keepDev,
          0, dstTop, ctx.canvas.width, keepDev,
        )
        ctx.restore()
      }
      // Back in CSS px for the strip. Rounded outward so a fractional viewport
      // height can only ever make us repaint a hair more than was uncovered,
      // never leave a sliver of stale pixels behind.
      if (kDev > 0) {
        stripTop = Math.floor((dstTop + keepDev) / blitScale)
        stripH = Math.ceil(H - stripTop)
      } else {
        stripTop = HEADER_H
        stripH = Math.ceil(-dy)
      }
      // Clear only what the blit did not cover. The header repaints its own
      // background (drawHeaderRow starts with a full-width fill), and everything
      // between is either blitted or in the strip.
      ctx.fillStyle = cPanel
      ctx.fillRect(0, stripTop, W, stripH)
    } else {
      // The panel colour is opaque and covers the whole canvas, so this single
      // fill also clears the previous frame - a separate clearRect would just be a
      // second full-surface pass (measurable on WebKitGTK's CPU-rendered canvas).
      ctx.fillStyle = cPanel
      ctx.fillRect(0, 0, W, H)
    }

    const usedW = Math.max(0, Math.min(W, geom.totalWidth - _scrollLeft))
    const navName = focusedCol !== null ? navigableColumns[focusedCol]?.name : null

    // Frozen left region (pinned cols only) - already summed by geometry.
    const frozenW = geom.frozenWidth

    // ── Body ─────────────────────────────────────────────────────────────
    ctx.save()
    ctx.beginPath()
    ctx.rect(0, HEADER_H, W, Math.max(0, H - HEADER_H))
    ctx.clip()
    ctx.textBaseline = 'middle'

    // Precompute the rectangular cell-range (in column-name space) once per frame
    // so drawCell can cheaply tint in-range cells and stroke the range border.
    const range = computeCellRange()
    /** @type {Set<string> | null} */
    let rangeColNames = null
    let rangeFirstCol = '', rangeLastCol = '', rangeR0 = -1, rangeR1 = -1
    if (range) {
      if (_rangeColNamesCache && _rangeColNamesC0 === range.c0 && _rangeColNamesC1 === range.c1 && _rangeColNamesCols === navigableColumns) {
        rangeColNames = _rangeColNamesCache
      } else {
        rangeColNames = new Set()
        for (let ci = range.c0; ci <= range.c1; ci++) {
          const nm = navigableColumns[ci]?.name
          if (nm) rangeColNames.add(nm)
        }
        _rangeColNamesCache = rangeColNames
        _rangeColNamesC0 = range.c0
        _rangeColNamesC1 = range.c1
        _rangeColNamesCols = navigableColumns
      }
      rangeFirstCol = navigableColumns[range.c0]?.name ?? ''
      rangeLastCol = navigableColumns[range.c1]?.name ?? ''
      rangeR0 = range.r0; rangeR1 = range.r1
    }

    // Frame-constant draw context - built ONCE per frame and shared by every
    // visible row, instead of a fresh ~20-field literal per row (that churned
    // thousands of short-lived objects/sec during scroll → GC jank).
    // Grid style preset + its dot size (integer, DPR-agnostic - canvas is already
    // scaled by canvasZoom), resolved once per frame.
    const tableStyle = _tableStyle
    const dotSize = Math.max(2, Math.round(2 * canvasZoom))

    // Vertical separator x-positions are identical for every row (they depend only
    // on columns + scroll, not the row), so collect them ONCE per frame here rather
    // than re-deriving them inside every drawBodyRow. Keeps the per-row grid pass to
    // a single loop over this array - flat regardless of how many million rows exist.
    // Reuse one buffer across frames so the scroll hot path allocates nothing here.
    const vSeps = _vSepsBuf
    vSeps.length = 0
    for (const col of geom.cols) {
      if (col.pinned) continue
      const dx = col.contentX - _scrollLeft
      if (dx >= W) break
      if (dx + col.w <= 0) continue
      const ex = dx + col.w - 0.5
      if (ex <= frozenW) continue
      vSeps.push(ex)
    }
    for (const vp of _vrelLayout) {
      const ex = vp.x + vp.w - _scrollLeft - 0.5
      if (ex <= frozenW || ex >= W) continue
      vSeps.push(ex)
    }
    for (const col of geom.cols) {
      if (!col.pinned) continue
      const ex = colDrawnX(col, geom, _scrollLeft) + col.w - 0.5
      if (ex <= 0 || ex >= W) continue
      vSeps.push(ex)
    }
    if (gutterWidth > 0) {
      const gex = (gutterWidth - _scrollLeft) - 0.5
      if (gex > 0 && gex < W) vSeps.push(gex)
    }

    // First non-pinned column not entirely left of the viewport - columns are
    // laid out sequentially (contentX ascending, adjacent), so the index depends
    // only on scroll, not the row. Computed once per frame so drawBodyRow never
    // re-scans the off-screen-left tail for every visible row.
    let firstColIdx = 0
    for (const col of geom.cols) {
      if (!col.pinned && col.contentX + col.w - _scrollLeft > 0) break
      firstColIdx++
    }

    // Pinned columns, collected once per frame. Null rather than an empty array
    // so drawBodyRow can skip the loop with a single check.
    /** @type {typeof geom.cols | null} */
    let pinnedCols = null
    for (const col of geom.cols) {
      if (!col.pinned) continue
      if (!pinnedCols) pinnedCols = []
      pinnedCols.push(col)
    }

    const bodyC = {
      cFg, cText, cMuted, cGrid, cBorder, cMutedBg, cRing, cAccent, cPanel, usedW, navName,
      // Capped cells only draw their Load control when there is somewhere to
      // load from, and the spinner's angle is shared by every cell in the frame.
      oversizeCells: !!onfetchcellvalue,
      loadingCells: _loadingCells,
      spinAngle: (performance.now() / 1000) * Math.PI * 1.6,
      AMBER, BLUE_FG, RED, cPrimary, frozenW, tableStyle, dotSize, vSeps, firstColIdx,
      rangeColNames, rangeFirstCol, rangeLastCol, rangeR0, rangeR1,
      // Checked rows, so the cell cursor can drop its side strokes on a row that
      // is already tinted end to end.
      selectedRows: selected,
      // Frame-constant snapshots of the reactive values the per-cell loops read.
      // Every entry below was previously read straight off the reactive graph
      // inside drawCell, i.e. once per cell per frame. A $derived read is not a
      // property access - it re-checks the derived against each of its
      // dependencies' write versions - and there were ~20 of them per cell, so a
      // 150-cell viewport paid ~3000 graph reads a frame for values that cannot
      // change mid-frame. Read once here, then drawCell only touches this object.
      cols: geom.cols, pinnedCols, scrollLeft: _scrollLeft, viewportWidth: W,
      padX: CELL_PAD_X, iconHit: ICON_HIT, zoom: canvasZoom, fonts: _fonts,
      highlightActiveRow: $appHighlightActiveRow,
      nameToActualIdx: _nameToActualIdx, colCache: _colCache,
      editingCell, focusedRow, hoveredRow, hoveredColName, focusColName, selectedCols,
      hasPendingEdits, pendingEdits, editedRowSet: _editedRowSet,
      colStats: _colStats, extActive: _extActive, colTransformFns: _colTransformFns,
      searchMatcher: _searchMatcher, nullishOn: _nullishOn, rows,
      alignAll: $appTableAlign === 'right',
    }

    const bodyTopY = Math.max(0, _scrollTop - HEADER_H - insertRowOffset)
    if (visibleColumns.length > 0) {
      let i = rowIndexAtY(rowTops, n, bodyTopY, ROW_HEIGHT)
      const firstVis = i
      let lastVis = i
      // On a blit frame every row outside the exposed strip is already on the
      // canvas, translated into place. A row straddling the strip edge is
      // cleared and repainted whole, which is cheaper than clipping and cannot
      // leave a seam. The loop
      // itself still runs to completion - emitVisibleRange drives row windowing
      // and must see the true visible range on every frame, blit or not.
      // Widened by a pixel at each edge: a row's separators and focus ring sit on
      // half-pixel coordinates and can bleed one pixel outside its own band, so
      // the neighbouring row is repainted too rather than risk a seam where the
      // blit met the strip. Worst case that is one extra row at each end.
      const repaintTop = stripTop - 1
      const repaintBot = stripTop + stripH + 1
      // Those rows are redrawn whole, so they are cleared whole first, not just
      // the strip. Text drawn over its own blitted copy doubles every
      // antialiased edge, and those rows came out brighter and bolder than the
      // rest, as if highlighted, wherever the strip edge happened to fall.
      if (canBlit) {
        let bandTop = Infinity
        let bandBot = -Infinity
        for (let j = i; j < n; j++) {
          const ry = rowViewportY(j)
          if (ry >= H || ry >= repaintBot) break
          if (ry + ROW_HEIGHT <= HEADER_H || ry + ROW_HEIGHT <= repaintTop) continue
          if (ry < bandTop) bandTop = ry
          if (ry + ROW_HEIGHT > bandBot) bandBot = ry + ROW_HEIGHT
        }
        if (bandBot > bandTop) {
          const top = Math.floor(bandTop)
          ctx.fillStyle = cPanel
          ctx.fillRect(0, top, W, Math.ceil(bandBot) - top)
        }
      }
      for (; i < n; i++) {
        const ry = rowViewportY(i)
        if (ry >= H) break
        if (ry + ROW_HEIGHT <= HEADER_H) continue
        if (!canBlit || (ry < repaintBot && ry + ROW_HEIGHT > repaintTop)) {
          drawBodyRow(ctx, i, ry, bodyC)
        }
        lastVis = i
      }
      if (n > 0) emitVisibleRange(firstVis, Math.min(lastVis, n - 1))
    }
    ctx.restore()

    // ── Header (pinned) - skip entirely when no columns visible ───────────
    if (visibleColumns.length > 0) {
      drawHeaderRow(ctx, {
        W, cPanel, cFg, cMuted, cGrid, cBorder, cMutedBg, cAccent, cPrimary, cRing,
        AMBER, AMBER_FG, BLUE_FG, RED, usedW,
        // The header's own `#` label reads `c.fonts.type`, exactly as the row
        // numbers under it do. Without this the row-number gutter threw
        // `undefined is not an object` on the first frame it was drawn - the
        // body context carried `fonts`, this one did not, and only the one
        // branch gated on the gutter being enabled ever touched it.
        fonts: _fonts,
      })
    }

    // Now that the visible set is known, drop the backlog the viewport has moved
    // past and start whatever decodes fit in the remaining slots.
    if (_imgQueue.length) pumpCellImages()
    // Keep the shimmer moving while skeleton rows are on screen, and only then.
    // Capped at ~30fps: it's a slow sweep across a few grey bars, so half the
    // frames look identical and the other half are free. The moment the windows
    // land, _sawSkeleton stays false and this loop ends by itself.
    if (_shimmerOn && _sawSkeleton) {
      const now = performance.now()
      if (now - _lastShimmerFrame > 32) {
        _lastShimmerFrame = now
        scheduleDraw()
      } else if (!_shimmerTimer) {
        _shimmerTimer = setTimeout(() => { _shimmerTimer = 0; scheduleDraw() }, 32)
      }
    }
    publishLoadingSpan()
  }

  // ── "What is it loading?" ─────────────────────────────────────────────────
  // Skeleton bars say *that* rows are coming, never which ones or how far along,
  // and on a million-row table a jump can leave a screenful of them up for a
  // second with nothing to read. The rows being waited on are already known here
  // - draw() paints them - so they're published to a pill instead of thrown away.
  /** First/last skeleton row painted this frame. */
  let _skelMin = -1
  let _skelMax = -1
  /** When the current run of skeleton frames began (0 = none on screen). */
  let _skelSince = 0
  let _spanPublished = 0
  /** @type {ReturnType<typeof setTimeout> | 0} */
  let _spanTimer = 0
  /** Rows drawn as skeletons, for the pill. @type {{first:number,last:number}|null} */
  let loadingSpan = $state(/** @type {{first:number,last:number}|null} */ (null))
  /** A window that lands quickly must not flash a pill on its way past. */
  const SPAN_SHOW_AFTER = 220
  /** `loading`, held back the same way: a fetch that lands within a frame or
   *  two never flashes a spinner. */
  let loadingVisible = $state(false)
  $effect(() => {
    if (!loading) { loadingVisible = false; return }
    const t = setTimeout(() => (loadingVisible = true), SPAN_SHOW_AFTER)
    return () => clearTimeout(t)
  })
  /** Paint holding. A table switch empties `columns` the moment it starts, and
   *  drawing that frame blanked the grid for the whole round trip - the old
   *  table, then nothing, then the new one. For the same grace window as the
   *  spinner the canvas keeps the last table's pixels and the swap lands as one
   *  frame. Never on a surface that holds nothing yet: a fresh or just-resized
   *  canvas is blank (or black in WebKit) and has to be painted. */
  const holdPaint = $derived(loading && columns.length === 0 && !loadingVisible)
  let _surfaceHasFrame = false
  /** Reactivity budget: the pill re-reads at most this often, not per frame. */
  const SPAN_PUBLISH_MS = 150

  /** Push (or clear) the skeleton span for the pill. Called at the end of every
   *  frame, so it stays off the per-cell path and costs one comparison. */
  function publishLoadingSpan() {
    if (!_sawSkeleton || !windowed) {
      _skelSince = 0
      if (loadingSpan) loadingSpan = null
      return
    }
    const now = performance.now()
    if (!_skelSince) _skelSince = now
    if (now - _skelSince < SPAN_SHOW_AFTER) {
      // Not yet worth showing. A repaint is already queued while the shimmer
      // animates; with reduced motion nothing else would come back to check.
      if (!_shimmerOn && !_spanTimer) {
        _spanTimer = setTimeout(() => { _spanTimer = 0; scheduleDraw() }, SPAN_SHOW_AFTER)
      }
      return
    }
    if (now - _spanPublished < SPAN_PUBLISH_MS) return
    _spanPublished = now
    if (loadingSpan?.first === _skelMin && loadingSpan?.last === _skelMax) return
    loadingSpan = { first: _skelMin, last: _skelMax }
  }

  /**
   * Deterministic 0..1 from a cell's coordinates.
   *
   * Deterministic matters twice over: a bar whose width changed between frames
   * would flicker, and a skeleton that reshuffles on every scroll frame reads as
   * the layout moving. Same cell, same width, every frame, until the real value
   * replaces it.
   */
  function cellNoise(/** @type {number} */ row, /** @type {number} */ col) {
    const h = Math.imul(row * 73856093 ^ col * 19349663, 0x45d9f3b)
    return ((h >>> 8) & 0xffff) / 0xffff
  }

  /** Fraction of a frame's sweep the shimmer band covers, in viewport widths. */
  const SHIMMER_BAND = 0.28
  /** One full left-to-right pass, in ms. */
  const SHIMMER_PERIOD = 1400
  /** True while any skeleton row was painted this frame - drives the shimmer loop. */
  let _sawSkeleton = false
  let _shimmerPhase = 0
  let _lastShimmerFrame = 0
  /** The frame's highlight gradient, built once and shared by every skeleton row.
   *  @type {CanvasGradient | null} */
  let _shimmerFill = null
  /** @type {ReturnType<typeof setTimeout> | 0} */
  let _shimmerTimer = 0
  /** Someone who asked for less motion gets static bars, not a sweep. */
  const _shimmerOn =
    typeof matchMedia !== 'function' || !matchMedia('(prefers-reduced-motion: reduce)').matches

  /**
   * Skeleton row for a window that hasn't loaded yet (windowed mode only).
   *
   * Built to be replaced without anything appearing to move: each bar sits on the
   * text baseline band, is as wide as a plausible value for that cell rather than a
   * flat 50%, and is flush right in the columns whose values will be flush right.
   * The old version drew identical half-width bars hard left in every column, so
   * the moment real rows landed every number jumped to the other side of its cell -
   * which is what read as the layout shifting.
   */
  function drawLoadingRow(ctx, idx, ry, rh, c) {
    if (c.tableStyle.zebra && (idx & 1)) {
      // Softer than a loaded row's stripe: a skeleton row is already busy with bars.
      ctx.fillStyle = withAlpha(c.cMutedBg, ZEBRA_ALPHA * 0.6)
      ctx.fillRect(0, ry, c.usedW, rh)
    }
    _sawSkeleton = true
    if (_skelMin < 0 || idx < _skelMin) _skelMin = idx
    if (idx > _skelMax) _skelMax = idx
    // Sit on the text's own band: cap height, centred where the glyphs will be, so
    // the bar and the value it becomes occupy the same pixels.
    const barH = Math.max(3, Math.round(rh * 0.28))
    const cy = ry + rh / 2
    const gut = geom.gutterWidth
    // Two passes so each opacity is one fill() for the whole row: the base bars,
    // then the highlight over the same path. Per-bar fills were the most
    // expensive thing on screen in exactly the state that has to feel smooth.
    ctx.beginPath()
    let any = false
    for (let ci = 0; ci < geom.cols.length; ci++) {
      const col = geom.cols[ci]
      const x = colDrawnX(col, geom, _scrollLeft)
      if (x + col.w <= gut || x >= _viewportWidth) continue
      const avail = col.w - CELL_PAD_X * 2
      if (avail <= 4) continue
      // 40-92% of the cell, stable per cell: text of varying length, not a barcode.
      const bw = Math.max(6, Math.round(avail * (0.4 + cellNoise(idx, ci) * 0.52)))
      const cellLeft = Math.max(x, gut)
      const bx = _skeletonRightAligned(col)
        ? x + col.w - CELL_PAD_X - bw
        : cellLeft + CELL_PAD_X
      if (bx + bw <= gut || bx >= _viewportWidth) continue
      roundRectPath(ctx, bx, cy - barH / 2, bw, barH, barH / 2)
      any = true
    }
    if (!any) return
    ctx.fillStyle = withAlpha(c.cMutedBg, 0.4)
    ctx.fill()
    if (!_shimmerOn) return
    // Same path again under the sweep. The gradient is transparent outside the
    // band, so bars away from it are untouched without testing them.
    if (!_shimmerFill) _shimmerFill = buildShimmerFill(ctx, c.cMutedBg)
    ctx.fillStyle = _shimmerFill
    ctx.fill()
  }

  /**
   * The frame's sweep, as one gradient every skeleton row fills through.
   *
   * Per-bar hit-testing against a band was what made this read as flicker rather
   * than shimmer: a bar was either lit or not, so it *stepped* between two
   * opacities, and every row stepped on the same frame - a hard-edged column of
   * bars blinking on and off. A gradient ramps each bar continuously instead, and
   * costs one allocation per frame rather than one per row.
   *
   * The axis is tilted a little so the highlight crosses the grid on a diagonal;
   * a perfectly vertical edge over aligned bars is what the eye reads as a seam.
   */
  function buildShimmerFill(ctx, mutedBg) {
    const band = Math.max(1, _viewportWidth * SHIMMER_BAND)
    // Travel a full band clear of both edges. The old sweep stopped with the band
    // still half on screen and jumped back to the left, so the highlight vanished
    // mid-grid once a period - a blink, exactly on the beat.
    const cx = -band + _shimmerPhase * (_viewportWidth + band * 2)
    const g = ctx.createLinearGradient(cx - band / 2, 0, cx + band / 2, band * 0.35)
    const peak = 0.34
    g.addColorStop(0, withAlpha(mutedBg, 0))
    g.addColorStop(0.35, withAlpha(mutedBg, peak * 0.45))
    g.addColorStop(0.5, withAlpha(mutedBg, peak))
    g.addColorStop(0.65, withAlpha(mutedBg, peak * 0.45))
    g.addColorStop(1, withAlpha(mutedBg, 0))
    return g
  }

  /** Whether a skeleton bar should hug the right edge, matching where the real
   *  value will sit. Uses the same name→index map drawCell does, so it costs a
   *  Map lookup rather than a scan of `columns` per bar per frame. @param {any} col */
  function _skeletonRightAligned(col) {
    const actualIdx = _nameToActualIdx.get(col.name) ?? -1
    return actualIdx >= 0 && isRightAlignedColumn(actualIdx)
  }

  /**
   * Full-row background tint for a body row, or null when the row draws bare.
   * One source of truth so pinned cells - which repaint an opaque panel band to
   * mask the scrolled content beneath them - can re-apply the same tint and the
   * highlight reads as one continuous row instead of stopping at the frozen edge.
   * `cMutedBg` is only a hair off `--panel` in dark themes, so the focused row
   * uses a faint primary tint (the focused *cell* layers 0.08 more on top).
   * @returns {string | null}
   */
  function rowBgStyle(idx, c) {
    // Red diff tint for rows staged for deletion.
    if (hasPendingDeletes && pendingDeletes.has(idx)) return withAlpha(c.RED, hoveredRow === idx ? 0.2 : 0.14)
    if (selected.has(idx)) return withAlpha(c.cPrimary, hoveredRow === idx ? 0.18 : 0.13)
    // The focused row's tint is what tells you where the keyboard is. Off by
    // choice for people who navigate by cell and find the full-width band noisy -
    // the focused CELL keeps its own ring either way, so nothing becomes invisible.
    if (focusedRow === idx && c.highlightActiveRow) return withAlpha(c.cPrimary, hoveredRow === idx ? 0.09 : 0.07)
    if (hoveredRow === idx) return withAlpha(c.cMutedBg, 0.18)
    // Zebra striping - a tint on odd rows. Below every interactive state above so
    // selection/hover/focus always win; O(1), no per-row allocation.
    if (c.tableStyle.zebra && (idx & 1)) return withAlpha(c.cMutedBg, ZEBRA_ALPHA)
    return null
  }

  /**
   * Alpha of the alternating-row tint, over `--muted`.
   *
   * Was 0.07, which in a dark theme put `--muted` a couple of values away from
   * `--panel` and made the stripe invisible - the Striped preset and the
   * "Alternating row colours" setting both looked like they did nothing. One
   * constant so the preset and the setting can never disagree.
   */
  const ZEBRA_ALPHA = 0.16

  /** Row tint for the row currently being drawn, hoisted so drawCell reads it for free. */
  let _rowBg = /** @type {string | null} */ (null)

  /** @param {CanvasRenderingContext2D} ctx */
  function drawBodyRow(ctx, idx, ry, c) {
    const rh = ROW_HEIGHT
    if (windowed && rows[idx] === undefined) { _rowBg = null; drawLoadingRow(ctx, idx, ry, rh, c); return }
    syncRowDisplayCache(idx)
    const isSel = selected.has(idx)
    const isPendingDelete = hasPendingDeletes && pendingDeletes.has(idx)
    _rowBg = rowBgStyle(idx, c)
    if (_rowBg) {
      ctx.fillStyle = _rowBg
      ctx.fillRect(0, ry, c.usedW, rh)
    }

    // Non-pinned cells. geom.cols is ordered by ascending contentX: start at the
    // frame's precomputed first visible index (c.firstColIdx) and break once a
    // column starts past the right viewport edge, so neither off-screen tail is
    // iterated per row (matters for very wide tables).
    // `cols`, `sl` and `vw` are read off the frame context rather than the
    // reactive graph: `geom` is a $derived, and `geom.cols.length` in the loop
    // condition re-entered it once per column per row.
    const cols = c.cols
    const sl = c.scrollLeft
    for (let ci = c.firstColIdx, nc = cols.length, vpw = c.viewportWidth; ci < nc; ci++) {
      const col = cols[ci]
      if (col.pinned) continue
      const dx = col.contentX - sl
      if (dx >= vpw) break
      if (dx + col.w <= 0) continue
      drawCell(ctx, idx, col, dx, ry, rh, c)
    }

    // Pinned cells on top (frozen left). c.pinnedCols is null when nothing is
    // pinned - the common case - so no row pays for a scan of every column.
    if (c.pinnedCols) {
      for (const col of c.pinnedCols) {
        drawCell(ctx, idx, col, colDrawnX(col, geom, sl), ry, rh, c, true)
      }
    }

    drawRowGutters(ctx, idx, -_scrollLeft, ry, rh, c)

    // Staged-delete decoration: a left red bar + a strikethrough across the row.
    if (isPendingDelete) {
      ctx.fillStyle = c.RED
      ctx.fillRect(0, ry, 2, rh)
      ctx.strokeStyle = withAlpha(c.RED, 0.7)
      ctx.lineWidth = 1
      const sy = Math.round(ry + rh / 2) + 0.5
      ctx.beginPath()
      ctx.moveTo(0, sy)
      ctx.lineTo(c.usedW, sy)
      ctx.stroke()
    }

    // Row ring when the row is focused + checked and the keyboard is on the ROW
    // rather than on a cell in it.
    //
    // It used to draw whenever the row was focused, on top of the focused cell's
    // own outline: a 1px stroke at ry+0.5 and a 2px stroke at ry+1, one pixel
    // apart, running the width of the row. That is what read as a broken border
    // - two teal lines with a sliver of row between them, stepping in and out
    // where the cell's strokes started and stopped, and doubling the rule the
    // row above already drew. One indicator at a time: the row tint says which
    // row, the cell outline says which cell, and the ring is for when there is
    // no cell to point at.
    if (focusedRow === idx && isSel && focusedCol === null) {
      ctx.strokeStyle = withAlpha(c.cPrimary, 0.45)
      ctx.lineWidth = 1
      ctx.strokeRect(0.5, ry + 0.5, c.usedW - 1, rh - 1)
    }

    // ── Virtual expression column cells ──────────────────────────────────────
    if (_vexprLayout.length > 0 && _fonts) {
      for (let vi = 0; vi < _vexprLayout.length; vi++) {
        const vc = _vexprLayout[vi]
        const cellX = vc.x - _scrollLeft
        if (cellX + vc.w <= 0 || cellX >= _viewportWidth) continue
        ctx.fillStyle = c.cPanel
        ctx.fillRect(cellX, ry, vc.w, rh)
        const isVHov = hoveredRow === idx && hoveredColName === vc.hoverKey
        if (isVHov) { ctx.fillStyle = withAlpha(c.cMutedBg, 0.15); ctx.fillRect(cellX, ry, vc.w, rh) }
        if (vi === 0) {
          ctx.strokeStyle = withAlpha(c.cPrimary, 0.18); ctx.lineWidth = 1
          ctx.beginPath(); ctx.moveTo(cellX + 0.5, ry + 4); ctx.lineTo(cellX + 0.5, ry + rh - 4); ctx.stroke()
        }
        const row = rows[idx]
        if (!row) continue
        const val = vexprText(idx, vc.fnIdx)
        const isUrl = looksLikeUrl(val)
        ctx.font = _fonts.cell
        ctx.textAlign = 'left'
        ctx.textBaseline = 'middle'
        ctx.fillStyle = isUrl ? withAlpha(c.cAccent, isVHov ? 1 : 0.85) : withAlpha(c.cFg, 0.75)
        const maxW = vc.w - 20
        ctx.fillText(truncText(ctx, val, maxW), cellX + 10, ry + rh / 2 + 0.5)
      }
    }

    // ── Virtual relationship column cells ─────────────────────────────────────
    // Drawn BEFORE the bottom grid line so the line renders on top of cell fills.
    for (let vi = 0; vi < virtualRelCols.length; vi++) {
      const vc = virtualRelCols[vi]
      const cw = _vrelLayout[vi].w
      const cellX = _vrelLayout[vi].x - _scrollLeft
      if (cellX + cw <= 0 || cellX >= _viewportWidth) continue
      // Panel base, then the row's own tint on top: painting only the panel left
      // the relationship columns dark while the rest of a selected row lit up,
      // so the highlight stopped mid-row.
      ctx.fillStyle = c.cPanel; ctx.fillRect(cellX, ry, cw, rh)
      if (_rowBg) { ctx.fillStyle = _rowBg; ctx.fillRect(cellX, ry, cw, rh) }
      const isActive = fkSubview?.rowIdx === idx && fkSubview?.kind === 'reverse' && fkSubview?.label === vc.label
      const isVHov = hoveredRow === idx && hoveredColName === _vrelHoverKeys[vi]
      const isFocusedRel = c.focusedRow === idx && c.navName === _vrelHoverKeys[vi]
      if (!_fonts) return

      // A relation is a link, so it draws as one: the link glyph, then the table
      // name as plain text. It was a bordered chip on every row - and the value
      // is the same for every row in the column, so a screenful of them was
      // thirty identical lozenges carrying one word of information between them.
      // The icon says "relation" once per cell; the text carries the name.
      const relFontPx = Math.max(10, _fonts.cellPx - 1)
      ctx.font = `500 ${relFontPx}px ${_fonts.family}`
      const relPadX = Math.round(10 * canvasZoom)
      const relIcon = Math.round(12 * canvasZoom)
      const relGap = Math.round(7 * canvasZoom)

      if (isActive) { ctx.fillStyle = withAlpha(c.cPrimary, 0.06); ctx.fillRect(cellX, ry, cw, rh) }
      else if (isFocusedRel) { ctx.fillStyle = withAlpha(c.cPrimary, 0.08); ctx.fillRect(cellX, ry, cw, rh) }

      drawIcon(
        ctx, 'link-2',
        cellX + relPadX, ry + (rh - relIcon) / 2, relIcon,
        isActive ? c.cPrimary : withAlpha(c.cMuted, isVHov ? 1 : 0.8),
        1.8,
      )
      const relTextX = cellX + relPadX + relIcon + relGap
      ctx.fillStyle = isActive ? c.cPrimary : withAlpha(c.cFg, isVHov ? 0.9 : 0.62)
      ctx.textBaseline = 'middle'; ctx.textAlign = 'left'
      ctx.fillText(truncText(ctx, vc.label, cw - (relTextX - cellX) - relPadX), relTextX, ry + rh / 2 + 0.5)

      // Same 2px inset box the cell cursor draws on a real column: one cursor,
      // whatever kind of column it is standing on.
      if (isFocusedRel) {
        ctx.strokeStyle = withAlpha(c.cPrimary, 0.9)
        ctx.lineWidth = 2
        ctx.strokeRect(cellX + 1.5, ry + 1.5, cw - 3, rh - 3)
      }
    }

    // ── Batched grid pass ───────────────────────────────────────────────────
    // All separators (column edges, virtual edges, gutter, bottom row line) are
    // collected into ONE path and stroked once - instead of a beginPath/stroke
    // per cell. This collapses ~(cols+3) draw-call flushes per row down to one,
    // the single biggest scroll-perf win alongside O(1) text measurement.
    const vw = c.viewportWidth
    // Vertical separators were collected once for the frame; the row only chooses
    // how to render them per the active grid-style preset. All branches stay a
    // single batched path/fill, so this is O(visible cols) no matter the row count.
    const ts = c.tableStyle
    const seps = c.vSeps
    // "Bordered" preset draws with the stronger border token for a high-contrast grid.
    const gridColor = ts.strong ? c.cBorder : c.cGrid

    if (ts.dots) {
      // "Connection dot" grid - a small square at each cell join (column separator
      // × the row's bottom edge) instead of full lines. One batched fill per row.
      const ds = c.dotSize
      const dy = ry + rh - ds
      const half = (ds / 2) | 0
      ctx.fillStyle = gridColor
      ctx.beginPath()
      ctx.rect(0, dy, ds, ds) // left edge join
      for (let k = 0; k < seps.length; k++) ctx.rect((seps[k] - half) | 0, dy, ds, ds)
      ctx.fill()
    } else {
      ctx.strokeStyle = gridColor
      ctx.lineWidth = 1
      if (ts.dash) ctx.setLineDash(ts.dash)
      ctx.beginPath()
      if (ts.cols) {
        // "Ticks" clips each column rule to a short mark at the row's foot - a
        // ruler rather than a cage. The column edge is still stated exactly where
        // it matters (against the row rule) without a full-height line per column.
        const colTop = ts.ticks ? ry + rh - Math.min(7, rh / 3) : ry
        for (let k = 0; k < seps.length; k++) { ctx.moveTo(seps[k], colTop); ctx.lineTo(seps[k], ry + rh) }
      }
      if (ts.rows) { ctx.moveTo(0, ry + rh - 0.5); ctx.lineTo(vw, ry + rh - 0.5) }
      // "Double" adds a second rule 3px inside the first. Batched into the SAME
      // path as the primary rules, so twin lines cost one stroke per row, not two.
      if (ts.double) {
        if (ts.cols) {
          for (let k = 0; k < seps.length; k++) { ctx.moveTo(seps[k] - 3, ry); ctx.lineTo(seps[k] - 3, ry + rh) }
        }
        if (ts.rows) { ctx.moveTo(0, ry + rh - 3.5); ctx.lineTo(vw, ry + rh - 3.5) }
      }
      ctx.stroke()
      if (ts.dash) ctx.setLineDash([]) // reset so other strokes stay solid

      // Group rule: a stronger line every Nth row, so you can count down a long
      // page without tracking a cursor across it - ledger paper's one good idea.
      // A second stroke, but only on 1 row in N, and only for the presets that
      // ask for it; the dash was reset above so this line is always solid.
      if (ts.groupEvery && (idx + 1) % ts.groupEvery === 0) {
        ctx.strokeStyle = c.cBorder
        ctx.beginPath()
        ctx.moveTo(0, ry + rh - 0.5)
        ctx.lineTo(vw, ry + rh - 0.5)
        ctx.stroke()
      }
    }
  }

  /** @param {CanvasRenderingContext2D} ctx */
  function drawCell(ctx, idx, col, cellX, ry, rh, c, pinned = false) {
    const w = col.w

    const actualIdx = c.nameToActualIdx.get(col.name) ?? -1
    const cached = c.colCache[actualIdx]

    if (pinned) {
      ctx.fillStyle = c.cPanel
      ctx.fillRect(cellX, ry, w, rh)
      // The opaque mask above wipes the row tint, so lay it back down.
      if (_rowBg) { ctx.fillStyle = _rowBg; ctx.fillRect(cellX, ry, w, rh) }
    }

    const ec = c.editingCell
    const editing = ec && ec.rowIdx === idx && ec.colIdx === actualIdx
    const staged = (c.hasPendingEdits && c.editedRowSet.has(idx)) ? c.pendingEdits.get(idx + ':' + actualIdx) : undefined
    const isDirty = !!staged
    const value = staged ? staged.value : c.rows[idx]?.[actualIdx]
    const isNull = value === null || value === undefined
    const isJson = !isNull && typeof value === 'object'
    const fk = cached?.fk ?? null
    const activeFk = fk && !isNull
    const isFocusedCell = c.focusedRow === idx && c.navName === col.name && !editing

    // Extension render directive (badges, tints, masks, links, swatches, …).
    // Computed once per cell and merged across enabled formatters; null/JSON
    // cells skip it entirely so the common path allocates nothing.
    let _ctxArg
    if (c.colStats) { _statsCtx.stats = c.colStats.get(actualIdx); _ctxArg = _statsCtx }
    const dir = (c.extActive && !isNull && !isJson)
      ? formatCellValue(value, cached?.colType ?? '', col.name, _ctxArg)
      : null

    // Cell background tints.
    if (!editing) {
      // Column-selection band (drawn first so other tints layer on top).
      if (c.selectedCols.has(col.name)) {
        ctx.fillStyle = withAlpha(c.cPrimary, 0.08); ctx.fillRect(cellX, ry, w, rh)
      }
      // Focused-column band (drawn first so per-cell tints layer on top).
      if (c.focusColName !== null && col.name === c.focusColName) {
        ctx.fillStyle = withAlpha(c.cPrimary, 0.1); ctx.fillRect(cellX, ry, w, rh)
      }
      if (isDirty) { ctx.fillStyle = withAlpha(c.AMBER, 0.15); ctx.fillRect(cellX, ry, w, rh) }
      else if (activeFk) { ctx.fillStyle = withAlpha(c.cAccent, 0.15); ctx.fillRect(cellX, ry, w, rh) }
      // The lone-focused-cell tint is skipped while a range is active so every
      // in-range cell (including the drag-end corner) shares one uniform block tint.
      else if (isFocusedCell && !c.rangeColNames) { ctx.fillStyle = withAlpha(c.cPrimary, 0.08); ctx.fillRect(cellX, ry, w, rh) }
      else if (dir?.bgTint) { ctx.fillStyle = dir.bgTint; ctx.fillRect(cellX, ry, w, rh) }

      // Rectangular range selection - tint every in-range cell + stroke the outer border.
      const inRange = c.rangeColNames && idx >= c.rangeR0 && idx <= c.rangeR1 && c.rangeColNames.has(col.name)
      if (inRange) {
        ctx.fillStyle = withAlpha(c.cPrimary, 0.16); ctx.fillRect(cellX, ry, w, rh)
        ctx.strokeStyle = withAlpha(c.cPrimary, 0.9)
        ctx.lineWidth = 1
        ctx.beginPath()
        if (idx === c.rangeR0) { ctx.moveTo(cellX, ry + 0.5); ctx.lineTo(cellX + w, ry + 0.5) }
        if (idx === c.rangeR1) { ctx.moveTo(cellX, ry + rh - 0.5); ctx.lineTo(cellX + w, ry + rh - 0.5) }
        if (col.name === c.rangeFirstCol) { ctx.moveTo(cellX + 0.5, ry); ctx.lineTo(cellX + 0.5, ry + rh) }
        if (col.name === c.rangeLastCol) { ctx.moveTo(cellX + w - 0.5, ry); ctx.lineTo(cellX + w - 0.5, ry + rh) }
        ctx.stroke()
      }
    } else {
      // Active edit cell - the DOM overlay draws its own ring-inset; no canvas
      // border needed here (a canvas strokeRect would bleed outside the cell on
      // the right/bottom edges and create a misaligned double-border with the DOM ring).
    }

    // (Vertical grid separators are batched once per row in drawBodyRow.)

    // Dirty inset marker.
    if (isDirty && !editing) {
      ctx.fillStyle = c.AMBER
      ctx.fillRect(cellX, ry, 2, rh)
    }

    if (editing) return // text drawn by the DOM overlay

    const rowHover = c.hoveredRow === idx
    const isHover = rowHover && c.hoveredColName === col.name
    const cy = ry + rh / 2

    // A per-column transform (chosen from the header menu) renders live and wins
    // over formatter directives; skipped for staged/editing cells.
    const colTf = (!staged && c.rows[idx]) ? c.colTransformFns[col.name] : undefined
    // Avatar / image thumbnail transform - draw the image itself, not text.
    if (colTf && _IMG_TF.has(colTf.id) && !isNull && _imagePreviewOn && isImageUrl(value)) {
      drawCellImage(ctx, String(value), cellX, ry, w, rh, cy, colTf.id === 'avatar', c)
      return
    }

    // Cell text - directive display wins; masked cells reveal on hover.
    const revealed = dir?.mask && isHover
    // SQL array columns render pgAdmin-style ({a,b}); jsonb arrays stay JSON.
    const isArrayCol = Array.isArray(value) && !!cached?.isArrayType
    const isVectorCol = typeof value === "string" && !!cached?.isVector
    // Geometry gets the same treatment as vectors: the type + SRID (+ point
    // coords) read better than a row of EWKT. geometrySummary only inspects
    // the header, so it is cheap enough for the draw path.
    const isGeomCol = typeof value === "string" && !!cached?.isGeom
    const text = colTf
      ? colTransformText(idx, actualIdx, value, colTf)
      : dir
        ? String(revealed ? (dir.reveal ?? dir.display) : (dir.display ?? displayCell(value)))
        : isArrayCol
          ? arrayDisplay(value)
          : isVectorCol
            ? vectorDisplay(value, vectorHeads(w))
            : isGeomCol
              ? geometrySummary(value)
              : staged || !c.rows[idx]
            ? displayCell(value)
            : cellDisplayText(idx, actualIdx, value)
    // NULL glyph when the Empty & NULL Markers plugin is on (formatters skip null).
    const shownText = (isNull && c.nullishOn) ? '∅' : text

    // Text color - directive link/fg may override (but never over a stronger
    // dirty/fk/focused state highlight).
    let textColor = isFocusedCell || isDirty || activeFk ? c.cFg
      : isNull ? c.cMuted
      : c.cText
    if (dir?.link) textColor = c.cAccent
    else if (dir?.fg && !isFocusedCell && !isDirty && !activeFk) textColor = dir.fg

    // Right-side content widths (sequential, no overlap).
    // Right-alignment applies to the plain-text path only. A swatch, dot or
    // status pill is a left-anchored object; sliding it to the right edge would
    // read as a layout bug rather than an alignment choice.
    const alignRight =
      !dir?.badge && !dir?.swatch && !dir?.dot && (cached ? cached.alignRight : c.alignAll)

    const warnW = dir?.warn ? Math.round(14 * c.zoom) : 0
    // The hover actions live on the side the value isn't using - right of
    // left-aligned text, left of right-aligned text - so they occupy empty
    // space in both cases. Only long values, the ones that would actually
    // collide, give up room, and only while the pointer is in the cell.
    const hoverW = isHover ? c.iconHit : 0
    const fkW = (activeFk && rowHover) ? 20 : 0
    // The same gap on both sides. Left-aligned text used to reserve 4px on the
    // assumption that a value never reaches the right edge - but a *truncated*
    // value reaches it every time, which put the ellipsis hard against the column
    // divider and made every long column read as congested.
    const rightReserve = c.padX + (alignRight ? 0 : hoverW) + fkW + warnW
    const hoverLeftReserve = alignRight ? hoverW : 0

    // Left-side decorations (color swatch / boolean dot) push the text right.
    let textX = cellX + c.padX
    let leftPad = 0
    if (dir?.swatch) {
      const sw = Math.round(11 * c.zoom)
      roundRect(ctx, textX, cy - sw / 2, sw, sw, Math.round(2.5 * c.zoom))
      ctx.fillStyle = dir.swatch; ctx.fill()
      ctx.strokeStyle = withAlpha(c.cBorder, 0.6); ctx.lineWidth = 1
      roundRect(ctx, textX, cy - sw / 2, sw, sw, Math.round(2.5 * c.zoom)); ctx.stroke()
      leftPad = sw + Math.round(7 * c.zoom)
    } else if (dir?.dot) {
      const dr = Math.round(7 * c.zoom)
      ctx.fillStyle = dir.dot
      ctx.beginPath(); ctx.arc(textX + dr / 2, cy, dr / 2, 0, Math.PI * 2); ctx.fill()
      leftPad = dr + Math.round(7 * c.zoom)
    }
    textX += leftPad + hoverLeftReserve
    const textMaxW = w - c.padX - leftPad - hoverLeftReserve - rightReserve

    ctx.font = c.fonts.cell
    ctx.textAlign = 'left'
    if (dir?.badge) {
      // Status pill - label inside a rounded, tinted capsule.
      const padX = Math.round(7 * c.zoom)
      const label = truncText(ctx, shownText, Math.max(0, textMaxW - padX * 2))
      const pillH = Math.min(rh - Math.round(6 * c.zoom), Math.round(17 * c.zoom))
      const pillW = Math.min(Math.max(0, textMaxW), textWidth(ctx, label) + padX * 2)
      const py = ry + (rh - pillH) / 2
      roundRect(ctx, textX, py, pillW, pillH, pillH / 2)
      ctx.fillStyle = dir.badge.bg; ctx.fill()
      ctx.fillStyle = dir.badge.fg
      ctx.fillText(label, textX + padX, cy + 0.5)
    } else {
      const drawn = truncText(ctx, shownText, Math.max(0, textMaxW))
      // Shift the left edge instead of flipping ctx.textAlign: the search
      // highlighter and the link underline below both measure from this x, so
      // one origin keeps all three in agreement.
      const drawX = alignRight
        ? textX + Math.max(0, textMaxW - textWidth(ctx, drawn))
        : textX
      if (c.searchMatcher && !isNull) drawSearchHighlights(ctx, drawn, drawX, ry, rh, c)
      ctx.fillStyle = textColor
      ctx.fillText(drawn, drawX, cy + 0.5)
      if (dir?.link) {
        const uy = cy + Math.round(7 * c.zoom)
        const uw = Math.min(textWidth(ctx, drawn), Math.max(0, textMaxW))
        ctx.strokeStyle = withAlpha(textColor, 0.5); ctx.lineWidth = 1
        ctx.beginPath(); ctx.moveTo(drawX, uy); ctx.lineTo(drawX + uw, uy); ctx.stroke()
      }
    }

    // Draw right-side items right-to-left with a running cursor.
    let rx = cellX + w - 4  // 4px right margin

    // 0. Validation warning marker (amber dot).
    if (dir?.warn) {
      const wr = Math.round(4 * c.zoom)
      ctx.fillStyle = withAlpha(c.AMBER, 0.95)
      ctx.beginPath(); ctx.arc(rx - wr, cy, wr, 0, Math.PI * 2); ctx.fill()
      rx -= warnW
    }

    // 1. Hover buttons. They go on whichever side the value is not using, so
    //    they land in empty space rather than over the text. Geometry must
    //    match cellButtonRects(), which is the click target for these.
    const capped = !!c.oversizeCells && isOversizeValue(value)
    // The key is only built when something is actually loading. Formatting one
    // per capped cell per frame is a string allocated 60 times a second to ask a
    // question whose answer is almost always "none of them".
    const loadingCell = capped && c.loadingCells.size > 0 && c.loadingCells.has(`${idx}:${actualIdx}`)
    if (isHover || loadingCell) {
      if (alignRight) {
        if (isHover) drawIcon(ctx, 'copy', cellX + 9, cy - 7, 14, c.cMuted, 1.8)
        if (capped) drawCellLoad(ctx, cellX + 9 + c.iconHit, cy, loadingCell, c)
      } else {
        if (isHover) drawIcon(ctx, 'copy', rx - c.iconHit + 5, cy - 7, 14, c.cMuted, 1.8)
        rx -= c.iconHit
        // The value is not here and this is how it arrives, so it draws whether
        // or not the row is hovered once it is fetching.
        if (capped) { drawCellLoad(ctx, rx - c.iconHit + 5, cy, loadingCell, c); rx -= c.iconHit }
      }
    }

    // 2. FK external-link icon (row hover only).
    if (activeFk && rowHover) {
      drawIcon(ctx, 'external-link', rx - 16, cy - 6, 12, withAlpha(c.cRing, 0.9), 2)
      rx -= 20
    }

    // No per-cell "JSON" pill: the column header already shows the type, and
    // drawing a braces icon + label (two ctx.font swaps + measureText + path)
    // on every JSON/JSONB cell every frame was pure clutter and the main scroll
    // bottleneck for JSON-heavy tables. JSON cells still open in the lightbox on
    // click (see the isJson hit-test) and show a maximize icon on hover.

    // Focused-cell outline - primary border, fully inset (no bleed to neighbours).
    // +1.5 offset keeps the 2px stroke's outer edge 0.5px inside the cell boundary
    // on all four sides, so left=top=right=bottom are visually symmetric.
    // Suppressed while a rectangular range is active - the range's own outline is
    // the selection indicator, and a per-cell ring on the drag-end cell reads as a
    // stray highlight inside the block.
    if (isFocusedCell && !c.rangeColNames) {
      // The same box whether or not the row is checked. The checked row used to
      // get top-and-bottom strokes instead, spanning the cell's full width and
      // butting into the row ring at both ends, so the cursor changed shape and
      // the row grew edges depending on a checkbox. The row ring now stands down
      // while a cell is focused (see drawBodyRow), which is what those two
      // half-measures were working around.
      ctx.strokeStyle = withAlpha(c.cPrimary, 0.9)
      ctx.lineWidth = 2
      ctx.strokeRect(cellX + 1.5, ry + 1.5, w - 3, rh - 3)
    }
  }

  /** @param {CanvasRenderingContext2D} ctx @param {number} offsetX scroll-adjusted left offset */
  function drawRowGutters(ctx, idx, offsetX, ry, rh, c) {
    if (gutterWidth <= 0) return
    const isPendingDelete = hasPendingDeletes && pendingDeletes.has(idx)
    ctx.fillStyle = c.cPanel
    ctx.fillRect(offsetX, ry, gutterWidth, rh)
    // Re-apply the row tint over the opaque band so the highlight (and the red
    // staged-delete diff) runs edge to edge instead of starting after the gutter.
    if (_rowBg) { ctx.fillStyle = _rowBg; ctx.fillRect(offsetX, ry, gutterWidth, rh) }
    let gx = offsetX
    if (showRowExpand) {
      if (isPendingDelete) {
        // A red minus in place of the expand chevron marks the staged deletion.
        ctx.strokeStyle = c.RED
        ctx.lineWidth = 2
        ctx.lineCap = 'round'
        const cx = gx + GUTTER_EXPAND_W / 2
        const cy = Math.round(ry + rh / 2) + 0.5
        ctx.beginPath()
        ctx.moveTo(cx - 5, cy)
        ctx.lineTo(cx + 5, cy)
        ctx.stroke()
      } else {
        const expanded = expandedRows.has(idx) || fkSubview?.rowIdx === idx
        const hov = hoveredRow === idx
        if (expanded || hov) {
          drawIcon(ctx, expanded ? 'chevron-down' : 'chevron-right',
            gx + (GUTTER_EXPAND_W - 14) / 2, ry + (rh - 14) / 2, 14,
            expanded ? c.cFg : c.cMuted, 2)
        }
      }
      gx += GUTTER_EXPAND_W
    }
    if (showSelection) {
      drawCheckbox(ctx, gx + (GUTTER_SELECT_W - 16) / 2, ry + (rh - 16) / 2, 16,
        { checked: selected.has(idx) },
        { border: c.cMuted, fill: c.cPrimary, mark: c.cPanel })
      gx += GUTTER_SELECT_W
    }
    if (GUTTER_NUM_W > 0) {
      // Right-aligned tabular digits: a column of numbers lines up on its last
      // digit or it reads as noise. Muted, and a shade brighter on the focused
      // row so the eye can find where it is without a second highlight.
      //
      // At the CELL size, not the type-annotation size: the gutter is a column
      // of values like any other, and two point sizes in one row read as two
      // different tables. Only the colour says it is chrome.
      ctx.font = c.fonts.cell
      ctx.textAlign = 'right'
      ctx.fillStyle = focusedRow === idx ? c.cFg : c.cMuted
      ctx.fillText(String(rowNumberOffset + idx + 1), gx + GUTTER_NUM_W - Math.round(7 * canvasZoom), ry + rh / 2 + 0.5)
      ctx.textAlign = 'left'
      gx += GUTTER_NUM_W
    }
    // (Gutter separator is batched once per row in drawBodyRow.)
  }

  /** @param {CanvasRenderingContext2D} ctx */
  function drawHeaderRow(ctx, c) {
    // Distinct header background - panel base + muted overlay for clear separation from body rows.
    ctx.fillStyle = c.cPanel
    ctx.fillRect(0, 0, c.W, HEADER_H)
    ctx.fillStyle = withAlpha(c.cMutedBg, 0.38)
    ctx.fillRect(0, 0, c.W, HEADER_H)

    // Non-pinned headers. Ordered by ascending contentX → break past the right edge.
    for (const col of geom.cols) {
      if (col.pinned) continue
      const dx = col.contentX - _scrollLeft
      if (dx >= c.W) break
      if (dx + col.w <= 0) continue
      drawHeaderCell(ctx, col, dx, c)
    }

    // Pinned headers.
    for (const col of geom.cols) {
      if (!col.pinned) continue
      drawHeaderCell(ctx, col, colDrawnX(col, geom, _scrollLeft), c)
    }

    if (gutterWidth > 0) {
      const gx0 = -_scrollLeft
      ctx.fillStyle = c.cPanel
      ctx.fillRect(gx0, 0, gutterWidth, HEADER_H)
      ctx.fillStyle = withAlpha(c.cMutedBg, 0.38)
      ctx.fillRect(gx0, 0, gutterWidth, HEADER_H)
      let gx = gx0
      if (showRowExpand) {
        if (expandedRows.size > 0) {
          drawIcon(ctx, 'chevrons-down-up', gx + (GUTTER_EXPAND_W - 14) / 2, (HEADER_H - 14) / 2, 14, c.cMuted, 2)
        }
        gx += GUTTER_EXPAND_W
      }
      if (showSelection) {
        drawCheckbox(ctx, gx + (GUTTER_SELECT_W - 16) / 2, (HEADER_H - 16) / 2, 16,
          { checked: allSelected, indeterminate: someSelected },
          { border: c.cMuted, fill: c.cPrimary, mark: c.cPanel })
        gx += GUTTER_SELECT_W
      }
      if (GUTTER_NUM_W > 0) {
        // Right-aligned like the numbers under it, so the column has one edge,
        // and at the header's own weight like every other column name.
        ctx.font = c.fonts.header
        ctx.textAlign = 'right'
        ctx.fillStyle = c.cMuted
        ctx.fillText('#', gx + GUTTER_NUM_W - Math.round(7 * canvasZoom), HEADER_H / 2 + 0.5)
        ctx.textAlign = 'left'
        gx += GUTTER_NUM_W
      }
      ctx.strokeStyle = c.cGrid
      ctx.lineWidth = 1
      ctx.beginPath(); ctx.moveTo(gx0 + gutterWidth - 0.5, 0); ctx.lineTo(gx0 + gutterWidth - 0.5, HEADER_H); ctx.stroke()
    }

    // Virtual expression column headers
    if (_vexprLayout.length > 0 && _fonts) {
      for (let vi = 0; vi < _vexprLayout.length; vi++) {
        const vc = _vexprLayout[vi]
        const vcKey = `__vcol__${vc.id}`
        const x = vc.x - _scrollLeft
        if (x + vc.w <= 0 || x >= c.W) continue
        // Header bg - slightly tinted to distinguish from real cols
        ctx.fillStyle = withAlpha(c.cMutedBg, resizingColName === vcKey ? 0.25 : 0.12)
        ctx.fillRect(x, 0, vc.w, HEADER_H)
        // Left accent line for first col
        if (vi === 0) {
          ctx.strokeStyle = withAlpha(c.cPrimary, 0.3); ctx.lineWidth = 2
          ctx.beginPath(); ctx.moveTo(x + 1, 4); ctx.lineTo(x + 1, HEADER_H - 4); ctx.stroke()
        }
        // Right border
        ctx.strokeStyle = c.cGrid; ctx.lineWidth = 1
        ctx.beginPath(); ctx.moveTo(x + vc.w - 0.5, 0); ctx.lineTo(x + vc.w - 0.5, HEADER_H); ctx.stroke()
        // "ƒ" indicator + name
        ctx.font = _fonts.header
        ctx.textAlign = 'left'
        ctx.textBaseline = 'middle'
        ctx.fillStyle = withAlpha(c.cPrimary, 0.5)
        ctx.fillText('ƒ', x + 8, HEADER_H / 2 + 0.5)
        const fW = ctx.measureText('ƒ ').width
        ctx.fillStyle = withAlpha(c.cFg, 0.7)
        ctx.fillText(truncText(ctx, vc.name, vc.w - fW - 24), x + 8 + fW, HEADER_H / 2 + 0.5)
        // Resize affordance
        if (_resizeHoverCol === vcKey || resizingColName === vcKey) {
          ctx.strokeStyle = withAlpha(c.cPrimary, 0.7); ctx.lineWidth = 2
          ctx.beginPath(); ctx.moveTo(x + vc.w - 1, 5); ctx.lineTo(x + vc.w - 1, HEADER_H - 5); ctx.stroke()
        }
      }
    }

    // Virtual relationship column headers
    for (let vi = 0; vi < virtualRelCols.length; vi++) {
      const vc = virtualRelCols[vi]
      const vrelKey = `__vrel__${vi}`
      const cw = _vrelLayout[vi].w
      const x = _vrelLayout[vi].x - _scrollLeft
      if (x + cw <= 0 || x >= c.W) continue
      ctx.fillStyle = withAlpha(c.cMutedBg, resizingColName === vrelKey ? 0.25 : 0.1)
      ctx.fillRect(x, 0, cw, HEADER_H)
      if (vi === 0) {
        ctx.strokeStyle = withAlpha(c.cPrimary, 0.25); ctx.lineWidth = 2
        ctx.beginPath(); ctx.moveTo(x + 1, 4); ctx.lineTo(x + 1, HEADER_H - 4); ctx.stroke()
      }
      ctx.strokeStyle = c.cGrid; ctx.lineWidth = 1
      ctx.beginPath(); ctx.moveTo(x + cw - 0.5, 0); ctx.lineTo(x + cw - 0.5, HEADER_H); ctx.stroke()
      if (!_fonts) continue
      // `CELL_PAD_X`, the same left edge every other column header starts at.
      // It used to be indented to the cell TEXT below it - past the link glyph -
      // which lined it up with its own values and out of line with every header
      // beside it. Headers read across the row; that edge wins.
      const hdrTextX = CELL_PAD_X
      ctx.font = _fonts.header; ctx.fillStyle = withAlpha(c.cMuted, 0.6)
      ctx.textAlign = 'left'; ctx.textBaseline = 'middle'
      ctx.fillText(truncText(ctx, vc.label, cw - hdrTextX - 12), x + hdrTextX, HEADER_H / 2 + 0.5)
      // Resize edge affordance (matches regular column behaviour)
      if (_resizeHoverCol === vrelKey || resizingColName === vrelKey) {
        ctx.strokeStyle = withAlpha(c.cPrimary, 0.7)
        ctx.lineWidth = 2
        ctx.beginPath(); ctx.moveTo(x + cw - 1, 5); ctx.lineTo(x + cw - 1, HEADER_H - 5); ctx.stroke()
      }
    }

    // Header bottom border - kept subtle (not a hard divider).
    ctx.strokeStyle = withAlpha(c.cBorder, 0.3)
    ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(0, HEADER_H - 0.5); ctx.lineTo(c.W, HEADER_H - 0.5); ctx.stroke()
  }

  /** @param {CanvasRenderingContext2D} ctx */
  function drawHeaderCell(ctx, col, x, c) {
    const w = col.w

    const sortInfo = _sortLookup.get(col.name)
    const sorted = !!sortInfo

    // Pinned headers are painted on top of scrolled columns - give them an opaque
    // backing so the columns sliding underneath don't bleed through.
    if (col.pinned) {
      ctx.fillStyle = c.cPanel; ctx.fillRect(x, 0, w, HEADER_H)
      ctx.fillStyle = withAlpha(c.cMutedBg, 0.38); ctx.fillRect(x, 0, w, HEADER_H)
    }

    // Persistent per-column highlight colour (chosen from the header menu).
    const _hl = colHighlights[col.name]
    const _hlHex = _hl?.color ? COL_HL_MAP.get(_hl.color) : undefined
    if (_hlHex) {
      ctx.fillStyle = withAlpha(_hlHex, 0.13); ctx.fillRect(x, 0, w, HEADER_H)
      ctx.fillStyle = withAlpha(_hlHex, 0.85); ctx.fillRect(x, HEADER_H - 2, w, 2)
    }

    // Per-column transform indicator: a subtle, thin bottom accent only. The old
    // full-cell primary wash tinted the whole header blue and read as noise; the
    // underline alone signals "this column's values are transformed" cleanly.
    if (colTransforms[col.name]) {
      const uh = Math.max(1, Math.round(1.5 * canvasZoom))
      ctx.fillStyle = withAlpha(c.cPrimary, 0.5)
      ctx.fillRect(x, HEADER_H - uh, w, uh)
    }

    // Background tint.
    if (resizingColName === col.name) { ctx.fillStyle = withAlpha(c.cPrimary, 0.08); ctx.fillRect(x, 0, w, HEADER_H) }
    else if (sorted) { ctx.fillStyle = withAlpha(c.cPrimary, 0.05); ctx.fillRect(x, 0, w, HEADER_H) }
    else if (hoveredColName === col.name && hoveredRow === null && _resizeHoverCol !== col.name) {
      ctx.fillStyle = withAlpha(c.cMutedBg, 0.25); ctx.fillRect(x, 0, w, HEADER_H)
    }

    // Focused-column highlight (toolbar "Jump to column") - tint + accent underline.
    if (focusColName === col.name) {
      ctx.fillStyle = withAlpha(c.cPrimary, 0.18); ctx.fillRect(x, 0, w, HEADER_H)
      ctx.fillStyle = withAlpha(c.cPrimary, 0.9); ctx.fillRect(x, HEADER_H - 2, w, 2)
    }

    // Column-selection highlight (shift+click range) - stronger tint + thick underline.
    if (selectedCols.has(col.name)) {
      ctx.fillStyle = withAlpha(c.cPrimary, 0.22); ctx.fillRect(x, 0, w, HEADER_H)
      ctx.fillStyle = c.cPrimary; ctx.fillRect(x, HEADER_H - 3, w, 3)
    }

    // Right grid separator.
    ctx.strokeStyle = withAlpha(c.cBorder, 0.25)
    ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(x + w - 0.5, 0); ctx.lineTo(x + w - 0.5, HEADER_H); ctx.stroke()

    const meta = colMeta.get(col.name)
    const cy = HEADER_H / 2
    // Reserve enough room on the right that the sort glyph keeps a clear margin
    // from both the edge and the type text.
    const SORT_ICON = 13
    const SORT_MARGIN_R = 9
    // A pinned column says so in its own header, left of the sort glyph. The
    // only sign before this was that the column stopped scrolling, which you
    // find out by scrolling - and the pin state is the one column setting with
    // no mark anywhere on the column it applies to.
    const PIN_ICON = 11
    const pinReserve = col.pinned ? PIN_ICON + 6 : 0
    const sortReserve = SORT_ICON + SORT_MARGIN_R + 4 + pinReserve
    ctx.textBaseline = 'middle'
    ctx.textAlign = 'left'

    // Relational indicators - a small amber key (PK) and blue link (FK) glyph
    // read more cleanly than lettered boxes and keep the colour coding.
    const indicators = []
    if (meta) {
      if (meta.pk) indicators.push({ icon: 'key-round', color: c.AMBER_FG })
      if (meta.fk) indicators.push({ icon: 'link-2', color: c.BLUE_FG })
    }
    const indReserve = indicators.length * 18

    // Required marker - a red asterisk on NOT NULL columns. The same mark a
    // required form field carries, so it reads without a legend, and it answers
    // "will this row insert" from the header instead of from the column menu. Its
    // width comes out of the name's budget rather than being painted over it, so a
    // long name still truncates against the right edge.
    ctx.font = _fonts.header
    const required = !!meta && !meta.nullable
    const reqW = required ? Math.ceil(textWidth(ctx, '*')) + Math.round(4 * canvasZoom) : 0

    // Column name - primary, medium weight. ALWAYS starts at CELL_PAD_X, the same
    // inset the cells below use, so every header lines up with every other header
    // and with its own column's values.
    const nameMaxW = w - CELL_PAD_X - sortReserve - indReserve - reqW - 8
    const name = truncText(ctx, col.name, Math.max(0, nameMaxW))
    ctx.fillStyle = withAlpha(c.cFg, sorted ? 1 : 0.9)
    ctx.fillText(name, x + CELL_PAD_X, cy + 0.5)
    let tx = x + CELL_PAD_X + textWidth(ctx, name)
    // The asterisk TRAILS the name. Leading it - which is what this used to do -
    // indented the name by its width on required columns only, so those headers
    // sat a few pixels right of every other header, and right of the values
    // underneath them. That mismatch is the misalignment; the mark itself is fine.
    if (required) {
      ctx.fillStyle = withAlpha(c.RED, 0.85)
      ctx.fillText('*', tx + Math.round(2 * canvasZoom), cy + 0.5)
      tx += reqW
    }
    tx += 7

    // PK / FK glyphs (vertically centred, accent-coloured).
    for (const ind of indicators) {
      drawIcon(ctx, ind.icon, tx, cy - 6.5, 13, ind.color, 1.5)
      tx += 18
    }

    // Optional column tag - a bordered chip after the name, tinted with the
    // column's highlight colour (or muted when none). The 1px hue border keeps the
    // chip crisp even when it sits on a same-hue header band. Only drawn with room.
    const _tag = _hl?.tag
    if (_tag) {
      ctx.font = _fonts.type
      const _tw = Math.ceil(textWidth(ctx, _tag))
      const _padX = 9, _pillH = 18, _pillW = _tw + _padX * 2, _r = _pillH / 2
      if (x + w - sortReserve - tx > _pillW + 8) {
        const _tagHex = _hlHex ?? c.cMuted
        const _py = Math.round(cy - _pillH / 2)
        // Soft filled badge (shadcn "secondary/destructive" style): a solid tinted
        // fill with hue text and no border - fully rounded.
        ctx.fillStyle = withAlpha(_tagHex, 0.2)
        roundRect(ctx, tx, _py, _pillW, _pillH, _r); ctx.fill()
        ctx.fillStyle = withAlpha(_tagHex, 1)
        ctx.textAlign = 'left'
        ctx.fillText(_tag, tx + _padX, cy + 0.5)
        tx += _pillW + 8
      }
    }

    // Inline datatype - secondary, lower contrast, a touch of breathing room.
    const typeStartX = tx + (indicators.length ? 5 : 3)
    const typeRoom = x + w - sortReserve - typeStartX
    if (typeRoom > 24 && col.dataType) {
      ctx.font = _fonts.type
      ctx.fillStyle = withAlpha(c.cMuted, 0.6)
      ctx.fillText(truncText(ctx, col.dataType, typeRoom), typeStartX, cy + 0.5)
    }

    // Pin glyph, just left of where the sort indicator sits.
    if (col.pinned) {
      drawIcon(
        ctx, 'pin',
        x + w - SORT_ICON - SORT_MARGIN_R - PIN_ICON - 4,
        cy - PIN_ICON / 2,
        PIN_ICON, withAlpha(c.cMuted, 0.75), 1.6,
      )
    }

    // Sort indicator - right-aligned with a clear margin, vertically centred.
    const sortX = x + w - SORT_ICON - SORT_MARGIN_R
    const sortY = cy - SORT_ICON / 2
    if (sorted) {
      const iconName = sortInfo.direction === 'asc' ? 'arrow-up' : 'arrow-down'
      drawIcon(ctx, iconName, sortX, sortY, SORT_ICON, withAlpha(c.cPrimary, 0.95), 1.7)
      // Multi-column sort: show this key's priority (1,2,3…) left of the arrow.
      if (sortInfo.total > 1) {
        ctx.font = `600 ${Math.round(9 * canvasZoom)}px ${_fonts.family}`
        ctx.textAlign = 'right'
        ctx.fillStyle = withAlpha(c.cPrimary, 0.95)
        ctx.fillText(String(sortInfo.index + 1), sortX - Math.round(1.5 * canvasZoom), cy + 0.5)
        ctx.textAlign = 'left'
      }
    } else if (_resizeHoverCol !== col.name && hoveredColName === col.name && hoveredRow === null) {
      drawIcon(ctx, 'chevrons-up-down', sortX, sortY, SORT_ICON, withAlpha(c.cMuted, 0.55), 1.6)
    }

    // Column annotator strip - mini histogram (numeric) or non-null fill bar.
    if (_colStats && annotatorEnabled()) {
      const st = _colStats.get(_nameToActualIdx.get(col.name) ?? -1)
      const stripH = Math.round(6 * canvasZoom)
      const sx = x + 4, sw = w - 8
      const sy = HEADER_H - stripH - 1
      if (st && sw > 8) {
        if (st.numeric && st.hist && st.max > st.min) {
          let maxCount = 0
          for (let i = 0; i < st.hist.length; i++) if (st.hist[i] > maxCount) maxCount = st.hist[i]
          if (maxCount > 0) {
            const bw = sw / st.hist.length
            ctx.fillStyle = withAlpha(c.cPrimary, 0.5)
            for (let i = 0; i < st.hist.length; i++) {
              if (!st.hist[i]) continue
              const bh = Math.max(1, (st.hist[i] / maxCount) * stripH)
              ctx.fillRect(sx + i * bw, sy + (stripH - bh), Math.max(1, bw - 1), bh)
            }
          }
        } else if (st.total > 0) {
          const ratio = (st.total - st.nulls) / st.total
          const barY = sy + stripH - 2
          ctx.fillStyle = withAlpha(c.cMutedBg, 0.5); ctx.fillRect(sx, barY, sw, 2)
          ctx.fillStyle = withAlpha(c.cPrimary, 0.45); ctx.fillRect(sx, barY, sw * ratio, 2)
        }
      }
    }

    // Resize-edge affordance.
    if (_resizeHoverCol === col.name || resizingColName === col.name) {
      ctx.strokeStyle = withAlpha(c.cPrimary, 0.8)
      ctx.lineWidth = 2
      ctx.beginPath(); ctx.moveTo(x + w - 1, 4); ctx.lineTo(x + w - 1, HEADER_H - 4); ctx.stroke()
    }
  }

  /** Size the canvas backing store (DPR-aware). Always sync CSS px size so macOS
   *  WebKit can't display a stale CSS box over a mismatched bitmap (blurry zoom).
   *  Called only from the (low-frequency) layout effect - interaction repaints go
   *  through the separate repaint effect - so it always re-applies the transform
   *  rather than short-circuiting on unchanged dimensions. That guarantees the
   *  visible canvas is transformed + painted on every mount / tab switch (a
   *  short-circuit here risked leaving a fresh canvas untransformed → blank). */
  /** Tallest the grid viewport has been - see the height in syncCanvasSurface. */
  let _tallestViewportH = 0
  /** Scale of the canvas transform, as applied by syncCanvasSurface (DPR, capped). */
  let _surfaceDpr = 1
  function syncCanvasSurface() {
    const canvas = canvasEl
    const probe = colorProbe
    if (!canvas || !probe) return false
    if (!_readColor) _readColor = createColorReader(probe)
    // ALWAYS (re)fetch the context for the CURRENT canvas element. getContext is
    // idempotent per element (returns the same object), so this is cheap - but if
    // Svelte recreated the <canvas> (it does across tab switches while this
    // instance persists), a cached _ctx would keep painting the OLD, detached
    // canvas and the visible one would stay black. Binding to canvasEl every call
    // keeps _ctx and the on-screen canvas in lockstep.
    _ctx = canvas.getContext('2d')
    const cssW = Math.max(1, Math.round(_viewportWidth))
    // Height is not simply the viewport's. Resizing the backing store on WebKit
    // swaps the new bitmap in a frame before the new box size lands, so the
    // related-rows dock opening (it shortens the viewport) showed one frame of
    // the grid stretched by old/new. So a shrink leaves the canvas as tall as
    // the viewport has been, clipped by the scroll container - but never taller
    // than the content, so it can never add scroll range the table did not
    // already have. Short tables still follow the viewport exactly.
    if (_viewportHeight > _tallestViewportH) _tallestViewportH = _viewportHeight
    const cssH = Math.max(1, Math.round(Math.max(_viewportHeight, Math.min(spacerHeight, _tallestViewportH))))
    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    _surfaceDpr = dpr
    const bw = Math.max(1, Math.round(cssW * dpr))
    const bh = Math.max(1, Math.round(cssH * dpr))
    canvas.style.width = cssW + 'px'
    canvas.style.height = cssH + 'px'
    // Only touch canvas.width/height when it actually changes - assigning clears
    // the canvas, and we want to avoid a redundant clear on every repaint.
    let resized = false
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw
      canvas.height = bh
      resized = true
      _surfaceHasFrame = false
    }
    // canvas.width/height assignment resets the transform, so always (re)apply it.
    if (_ctx) _ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    return { ok: !!_ctx, resized }
  }

  // ── Crash containment ───────────────────────────────────────────────────
  // The draw loop and canvas event handlers run outside Svelte's effect tree
  // (rAF callbacks / DOM events), so an exception there bypasses the tab's
  // <svelte:boundary> - the grid would freeze with no error UI and keep
  // throwing every frame. Capture the first such error and rethrow it from an
  // effect, which IS inside the boundary: the per-tab error card ("Reload this
  // view") takes over this tab only, and the rest of the app stays alive.
  let _fatalError = $state(/** @type {unknown} */ (null))
  function reportFatal(/** @type {unknown} */ err) {
    console.error('[DataTable] fatal error:', err)
    if (_fatalError === null) _fatalError = err ?? new Error('Unknown table error')
  }
  $effect(() => {
    const e = _fatalError
    if (e) throw e instanceof Error ? e : new Error(String(e))
  })
  /** Wrap a canvas event handler so a crash fails this tab, not the app.
   * @template {(...args: any[]) => any} F @param {F} fn @returns {F} */
  function guarded(fn) {
    return /** @type {F} */ ((/** @type {any[]} */ ...args) => {
      try {
        return fn(...args)
      } catch (err) {
        reportFatal(err)
      }
    })
  }

  // rAF-batched paint - coalesces bursts of scroll/state changes into a single
  // draw per animation frame so high-frequency trackpad scroll (up to 120Hz on
  // ProMotion) never queues multiple synchronous repaints and tears/janks.
  let _drawRafId = 0
  function scheduleDraw() {
    // While the scroll loop owns the frame, hand the request to it instead of
    // scheduling a second rAF - both call draw(), and letting them race painted
    // the entire canvas twice per frame during scrolling.
    if (_scrollLoopId) { _loopNeedsDraw = true; return }
    if (_drawRafId) return
    _drawRafId = requestAnimationFrame(() => {
      _drawRafId = 0
      if (!_ctx || _fatalError) return
      try {
        draw()
      } catch (err) {
        reportFatal(err)
      }
    })
  }
  onDestroy(() => {
    if (_drawRafId) cancelAnimationFrame(_drawRafId)
    if (_resizeRafId) cancelAnimationFrame(_resizeRafId)
    if (_scrollLoopId) cancelAnimationFrame(_scrollLoopId)
    if (_focusColTimer) { clearTimeout(_focusColTimer); _focusColTimer = null }
    // Thumbnails hold GPU/heap memory that is not reclaimed by dropping the
    // component, and pending retry timers would fire against a dead canvas.
    releaseCellImages()
    if (_shimmerTimer) { clearTimeout(_shimmerTimer); _shimmerTimer = 0 }
    if (_spanTimer) { clearTimeout(_spanTimer); _spanTimer = 0 }
    // Remove any window resize listeners still attached from a drag in progress.
    clearActiveResizeListeners()
  })

  // Shift+wheel → horizontal scroll. Non-passive so we can call preventDefault,
  // but bails out immediately on non-Shift events so the compositor waits <0.05ms.
  // Wheel handling is split so PLAIN vertical scrolling stays on the compositor
  // (buttery, no per-tick main-thread round-trip). A *non-passive* wheel listener
  // - needed to preventDefault ctrl-zoom and shift-horizontal - otherwise forces
  // the browser to consult JS before every scroll tick, and while the redraw loop
  // is busy that round-trip lands late → the exact stutter reported even on tiny
  // tables. So the non-passive listener is attached ONLY while Ctrl/Shift/Alt is
  // physically held; the rest of the time there is no blocking wheel listener at
  // all and the OS scrolls the container directly.
  // With eased scrolling on (the default - Settings → Appearance → Native
  // scrolling), the listener has to be live for EVERY tick, because the offset is
  // animated here rather than by the OS. That is the cost the setting exists to
  // let people opt out of: turning native scrolling on removes this listener
  // except while Ctrl/Shift is held, restoring the compositor-driven path exactly
  // as described above.
  $effect(() => {
    const el = tableContainer
    if (!el) return
    const easedScroll = !$appNativeScroll
    // Accumulates pinch/ctrl-wheel delta so many small gesture ticks map to whole
    // app-zoom steps instead of one step per event.
    let _zoomAccum = 0
    let _activeAttached = false
    const scroller = easedScroll ? createSmoothScroll(el) : null

    function doZoom(/** @type {number} */ deltaY) {
      _zoomAccum += deltaY
      while (_zoomAccum <= -24) { increaseZoom(); _zoomAccum += 24 }
      while (_zoomAccum >= 24) { decreaseZoom(); _zoomAccum -= 24 }
    }

    // Alt+wheel steps PRECISE_ROWS rows per notch. On a multi-million-row table the scroll
    // range is compressed (`_scrollScale` > 1), so a plain notch travels dozens of
    // rows; this is the way to land on a specific one.
    // The target is kept as a float in VIRTUAL space rather than read back from
    // scrollTop: in scaled mode one row is a fractional number of physical pixels,
    // and the element rounds what it's given, so `scrollTop += step` would drift a
    // little every notch.
    const PRECISE_ROWS = 4
    let _preciseVirt = -1
    let _preciseAccum = 0
    function doPreciseStep(/** @type {number} */ dy, /** @type {boolean} */ notch) {
      // A trackpad sends many small pixel deltas: PRECISE_ROWS rows per notch's
      // worth (~50px) of travel. A mouse notch (line mode, or one coarse pixel
      // tick) is exactly PRECISE_ROWS rows.
      let rows = 0
      if (notch) rows = Math.sign(dy) * PRECISE_ROWS
      else {
        const per = 50 / PRECISE_ROWS
        _preciseAccum += dy
        rows = Math.trunc(_preciseAccum / per)
        _preciseAccum -= rows * per
      }
      if (!rows) return
      // Resync when something else moved the grid since our last step (plain
      // wheel, scrollbar, keyboard). Snap to a row boundary so each step leaves a
      // whole row under the header; only valid while every row is ROW_HEIGHT tall.
      if (_preciseVirt < 0 || Math.abs(_preciseVirt - _scrollTop) > _scrollScale + 1) {
        _preciseVirt = _scrollTop
        if (expandedRows.size === 0) {
          _preciseVirt = insertRowOffset + Math.round((_preciseVirt - insertRowOffset) / ROW_HEIGHT) * ROW_HEIGHT
        }
      }
      const maxVirt = Math.max(0, contentHeight - _viewportHeight)
      _preciseVirt = Math.min(maxVirt, Math.max(0, _preciseVirt + rows * ROW_HEIGHT))
      scroller?.stop()
      el.scrollTop = virtToPhys(_preciseVirt)
      scroller?.sync()
    }

    // Non-passive: live for every tick under eased scrolling, otherwise only while
    // a modifier that needs preventDefault is down.
    const onWheelActive = (/** @type {WheelEvent} */ e) => {
      if (e.ctrlKey) { e.preventDefault(); doZoom(e.deltaY); return }
      if (e.altKey && !e.shiftKey && !e.metaKey) {
        const { dy } = wheelPixels(e, el.clientHeight)
        if (!dy) return
        e.preventDefault()
        doPreciseStep(dy, e.deltaMode !== 0 || Math.abs(dy) >= 40)
        return
      }
      // If the pointer is over a nested horizontally-scrollable panel (the FK
      // sub-view), scroll that instead of the main grid.
      const inner = e.target instanceof Element
        ? e.target.closest('[data-fk-subview-scroll]')
        : null
      if (e.shiftKey) {
        // Normalize FIRST. A mouse that reports deltaMode 1 (lines) sends ±1..3,
        // and treating that as pixels moved the grid by three pixels a notch -
        // horizontal scrolling looked broken under eased scrolling while native
        // mode looked fine, because there the browser does this conversion itself.
        const { dx, dy } = wheelPixels(e, el.clientWidth)
        const delta = dy || dx // shift + vertical wheel IS horizontal movement
        if (!delta) return
        if (inner && inner !== el && inner.scrollWidth > inner.clientWidth) {
          e.preventDefault()
          inner.scrollLeft += delta
          return
        }
        e.preventDefault()
        if (scroller) scroller.push(delta, 0)
        else el.scrollLeft += delta
        return
      }
      if (!scroller) return
      // Plain wheel, eased: animate the offset ourselves. The rAF redraw loop
      // still runs off the resulting `scroll` events, so nothing else changes.
      if (inner && inner !== el) return
      const { dx, dy } = wheelPixels(e, el.clientHeight)
      if (!dy && !dx) return
      // A trackpad's horizontal swipe arrives as plain deltaX (no Shift).
      if (dy && el.scrollHeight > el.clientHeight) {
        e.preventDefault()
        scroller.push(0, dy)
      }
      if (dx && el.scrollWidth > el.clientWidth) {
        e.preventDefault()
        scroller.push(dx, 0)
      }
    }

    // Passive fallback (always on, never blocks scroll): catches a trackpad pinch,
    // which arrives as a synthetic ctrl+wheel with NO physical Ctrl keydown, so the
    // gated listener above isn't attached for it. Drives the app's own zoom without
    // preventDefault - native page-zoom is already blocked (macOS
    // setAllowsMagnification / Tauri zoom_hotkeys_enabled=false). Skips when the
    // active listener is attached so a real Ctrl+wheel isn't handled twice.
    const onWheelPassive = (/** @type {WheelEvent} */ e) => {
      if (_activeAttached) return
      if (e.ctrlKey) doZoom(e.deltaY)
    }

    function attach() {
      if (_activeAttached) return
      el.addEventListener('wheel', onWheelActive, { passive: false })
      _activeAttached = true
    }
    function detach() {
      if (!_activeAttached) return
      el.removeEventListener('wheel', onWheelActive)
      _activeAttached = false
    }
    // Under eased scrolling the listener stays attached; the modifier gating below
    // is what keeps it OFF the hot path when the OS is doing the scrolling.
    /** @param {KeyboardEvent} e */
    const onKey = (e) => {
      if (easedScroll) return
      if (e.ctrlKey || e.shiftKey || e.altKey) attach()
      else detach()
    }
    // A keyboard scroll, scrollIntoView, or a scrollbar drag moved the element
    // without us: adopt the new position so the next wheel tick eases from there.
    const onScrollSync = () => { if (scroller && !scroller.animating()) scroller.sync() }
    // Blur drops the modifier-gated listener because a keyup that happens while
    // another window has focus never reaches us. Under eased scrolling the
    // listener is not modifier-gated and must survive - detaching it there would
    // leave the grid unable to scroll until the component remounted.
    const onBlur = () => { if (!easedScroll) detach() }

    // A press interrupts coasting - grabbing the scrollbar, or clicking a cell
    // while the tail of an ease is still running, takes effect now.
    const onInterrupt = () => { scroller?.stop(); scroller?.sync() }

    el.addEventListener('wheel', onWheelPassive, { passive: true })
    if (easedScroll) {
      attach()
      el.addEventListener('scroll', onScrollSync, { passive: true })
      el.addEventListener('pointerdown', onInterrupt, { passive: true })
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('keyup', onKey, true)
    window.addEventListener('blur', onBlur)
    return () => {
      scroller?.stop()
      detach()
      el.removeEventListener('pointerdown', onInterrupt)
      el.removeEventListener('scroll', onScrollSync)
      el.removeEventListener('wheel', onWheelPassive)
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('keyup', onKey, true)
      window.removeEventListener('blur', onBlur)
    }
  })

  // Canvas element binding - Svelte can recreate the <canvas> across tab switches
  // while this component instance (and its cached _ctx) persist. When the element
  // identity changes, immediately re-bind the context, re-apply size + transform,
  // and repaint the NEW element - otherwise draws keep hitting the old, detached
  // canvas and the visible grid stays black. Tracks canvasEl only.
  $effect(() => {
    const el = canvasEl
    if (!el) { _ctx = null; return }
    if (_ctx?.canvas === el) return // already bound to this element
    const { ok } = syncCanvasSurface()
    if (ok) draw()
  })

  // First-paint guard for the layout effect below (see there).
  let _firstPaintDone = false
  let _lastLayoutVh = 0
  // Layout / sizing effect - resize the backing store when geometry or viewport
  // changes. Tracks ONLY dependencies that can change the canvas dimensions,
  // geometry, or the full set of drawn data. Interaction state (selection, focus,
  // edit, hover, staged edits) is intentionally NOT tracked here - it can never
  // change canvas dimensions, so it lives in the lightweight repaint effect below.
  // Keeping it out means arrow-key nav and drag-select don't re-run the resurface
  // dependency graph or touch the backing store on every tick.
  $effect(() => {
    void rows; void columns; void columnWidths
    void pinnedColumns; void hiddenColumns
    void rowSort; void _viewportWidth
    // NOTE: _scrollTop / _scrollLeft are intentionally NOT tracked here - scrolling
    // repaints via scheduleDraw() inside onContainerScroll, so this effect (which
    // also re-syncs the canvas backing store) doesn't run on every scroll frame.
    void _viewportHeight; void newRowDrafts; void colMeta
    void geom; void rowTops; void _redrawToken; void foreignKeys; void indexes
    void _colCache; void expandedRows; void expandedRowHeights; void virtualRelCols; void _vrelLayout
    void vexprTotalW; void _vcolFns
    void zoomState.value; void canvasZoom
    const { ok, resized } = syncCanvasSurface()
    // The canvas can outgrow the viewport (see syncCanvasSurface), and what sits
    // below the fold is stale - a dock closing has to paint it in the same frame.
    const vhChanged = _viewportHeight !== _lastLayoutVh
    _lastLayoutVh = _viewportHeight
    if (ok) {
      // Paint synchronously only on the FIRST mount (so a fresh canvas never
      // composites as an untransformed, unpainted "black" grid) and whenever the
      // backing store was actually resized (assigning canvas.width/height clears
      // it - without a sync repaint the grid would blank for a frame). Every other
      // structural change leaves the last frame's pixels intact, so the
      // rAF-coalesced scheduleDraw() below is enough and the previously
      // unconditional synchronous draw() (which double-painted every update) is
      // skipped.
      if (!_firstPaintDone || resized || vhChanged) draw()
      _firstPaintDone = true
      scheduleDraw()
    }
  })

  // Repaint effect - interaction/visual state that changes what's drawn but never
  // the canvas dimensions. A plain scheduleDraw() (rAF-coalesced) with no backing-
  // store work, so hover/selection/focus/edit stay cheap even during drag-select.
  $effect(() => {
    void hoveredRow; void hoveredColName; void _resizeHoverCol; void resizingColName
    void selected; void focusedRow; void focusedCol; void selAnchor; void editingCell
    void pendingEdits; void pendingDeletes; void insertSaving; void fkSubview
    void holdPaint
    if (_ctx) scheduleDraw()
  })

  // ── Canvas pointer interaction ──────────────────────────────────────────
  // The canvas element doesn't move while the pointer hovers - only scrolling,
  // resizing or zooming can shift it. So we cache its bounding rect and reuse it
  // across the many pointermove events, instead of forcing a layout reflow with
  // getBoundingClientRect() on every single move. Invalidated on scroll/resize.
  /** @type {DOMRect | null} */
  let _canvasRect = null
  function invalidateCanvasRect() { _canvasRect = null }
  function canvasXY(/** @type {{ clientX: number, clientY: number }} */ e) {
    let r = _canvasRect
    if (!r) {
      r = canvasEl?.getBoundingClientRect() ?? null
      _canvasRect = r
    }
    if (!r) return { x: 0, y: 0 }
    return { x: e.clientX - r.left, y: e.clientY - r.top }
  }

  /**
   * Hit-test a viewport point.
   * @returns {{ kind: string, idx?: number, col?: any, actualIdx?: number, drawnX?: number, x?: number, y?: number }}
   */
  function hitTest(x, y) {
    // Content x accounts for scroll - gutters live in content space, not frozen.
    const cx = x + _scrollLeft
    if (y < HEADER_H) {
      if (cx < gutterWidth) {
        if (showRowExpand && cx < GUTTER_EXPAND_W) return { kind: 'header-expand-all' }
        return { kind: 'header-select-all' }
      }
      const hit = colAtX(x, geom, _scrollLeft, 0)
      if (!hit) return { kind: 'none' }
      return { kind: 'header', col: hit.col, drawnX: hit.drawnX, x, y }
    }
    const bodyY = y + _scrollTop - HEADER_H - insertRowOffset
    const r = rowAtContentY(rowTops, rows.length, ROW_HEIGHT, bodyY)
    if (!r || !r.inRowBody) return { kind: 'none' }
    const idx = r.idx
    if (cx < gutterWidth) {
      if (showRowExpand && cx < GUTTER_EXPAND_W) return { kind: 'row-expand', idx }
      return { kind: 'row-select', idx }
    }
    const hit = colAtX(x, geom, _scrollLeft, 0)
    if (!hit) return { kind: 'none', idx }
    const actualIdx = _nameToActualIdx.get(hit.col.name) ?? -1
    return { kind: 'cell', idx, col: hit.col, actualIdx, drawnX: hit.drawnX, x, y }
  }

  function onCanvasClick(/** @type {MouseEvent} */ e) {
    if (e.button !== 0) return
    // The cell is under the pointer, so it is on screen already: nothing here
    // may scroll the column it lands on.
    _focusFromKey = false
    if (_suppressNextClick) { _suppressNextClick = false; return }
    const { x, y } = canvasXY(e)

    // Check virtual expression column clicks
    if (y >= HEADER_H && _vexprLayout.length > 0) {
      const cx = x + _scrollLeft
      for (let vi = 0; vi < _vexprLayout.length; vi++) {
        const vc = _vexprLayout[vi]
        if (cx >= vc.x && cx < vc.x + vc.w) {
          const bodyY = y + _scrollTop - HEADER_H - insertRowOffset
          const r = rowAtContentY(rowTops, rows.length, ROW_HEIGHT, bodyY)
          if (!r?.inRowBody) return
          const row = rows[r.idx]
          if (!row) return
          const fn = _vcolFns[vc.fnIdx]
          const val = fn ? fn(row) : ''
          if (looksLikeUrl(val) && $appOpenUrlsOnClick) {
            void import('@tauri-apps/plugin-opener').then(m => m.openUrl(val)).catch(() => {})
          } else if (val) {
            void navigator.clipboard.writeText(val).catch(() => {})
          }
          return
        }
      }
    }
    // Check virtual relationship column clicks (right of real columns)
    const relHit = vrelHitAt(x, y)
    if (relHit) {
      if (relHit.rowIdx >= 0) {
        // Modifier click follows the relation, the way it does on a foreign key:
        // ⌘/Ctrl opens the related rows as a view, ⇧ puts them in a new tab.
        // Plain click keeps the dock preview.
        if (e.metaKey || e.ctrlKey || e.shiftKey) {
          openReverseFkFullView(relHit.rowIdx, relHit.vc, {
            newTab: e.shiftKey && !(e.metaKey || e.ctrlKey),
          })
        } else {
          toggleReverseFkSubview(relHit.rowIdx, relHit.vc)
        }
      }
      return
    }

    const t = hitTest(x, y)
    switch (t.kind) {
      case 'header-expand-all': collapseAllRows(); return
      case 'header-select-all': toggleAll(!allSelected); return
      case 'header': {
        if (e.shiftKey) {
          // Shift+click adds/toggles this column as a SECONDARY sort key
          // (multi-column sort), the standard data-grid gesture.
          handleHeaderSort(t.col.name, true)
        } else {
          selectedCols = new Set([t.col.name])
          _lastHeaderClickedCol = t.col.name
          handleHeaderSort(t.col.name, false)
        }
        scheduleDraw()
        return
      }
      case 'row-expand': toggleRowExpand(/** @type {number} */ (t.idx)); return
      case 'row-select': handleRowSelect(/** @type {number} */ (t.idx), e.shiftKey); return
      case 'cell': {
        const idx = /** @type {number} */ (t.idx)
        const actualIdx = /** @type {number} */ (t.actualIdx)

        if (e.shiftKey && selectedCols.size && t.col) {
          // Shift+click body cell while cols selected → extend column range.
          extendColSelection(t.col.name)
          scheduleDraw()
          return
        }
        if (selectedCols.size) {
          selectedCols = new Set()
          _lastHeaderClickedCol = null
          scheduleDraw()
        }
        if (editingCell) cancelEdit()

        // Shift+click previews the cell in the dock - the pointer half of
        // Space. A cell you have to squint at is the reason the dock
        // exists, and reaching for a chord to open it is a step.
        if (e.shiftKey) {
          e.stopPropagation()
          focusedRow = idx
          const vis = actualToVisColIdx(actualIdx)
          if (vis >= 0) focusedCol = vis
          openCellEditor(idx, actualIdx)
          return
        }

        clearCellRange() // plain click collapses any rectangular range
        focusedRow = idx
        const vi = actualToVisColIdx(actualIdx)
        if (vi >= 0) focusedCol = vi
        if (inspectorRow !== null) inspectorRow = idx

        const cached = _colCache[actualIdx]
        const value = effectiveCellValue(idx, actualIdx)
        const isNull = value === null || value === undefined
        const isJson = !isNull && typeof value === 'object'
        // Same alignment test the draw pass uses, so the click target follows
        // the button to whichever side it was painted on.
        const cappedCell = isOversizeValue(value)
        const { copy, load } = cellButtonRects(
          /** @type {number} */ (t.drawnX), t.col.w, 0, ROW_HEIGHT,
          { alignRight: isRightAlignedColumn(actualIdx), withLoad: cappedCell && !!onloadcellvalue },
        )
        const relX = x - /** @type {number} */ (t.drawnX)
        if (load && relX >= load.x - /** @type {number} */ (t.drawnX) && relX <= load.x - /** @type {number} */ (t.drawnX) + load.w) {
          e.stopPropagation(); void loadCellInline(idx, actualIdx); return
        }
        if (relX >= copy.x - /** @type {number} */ (t.drawnX) && relX <= copy.x - /** @type {number} */ (t.drawnX) + copy.w) {
          void copyCellValue(idx, actualIdx); return
        }
        // A JSON cell opens in the bottom dock, tree-first. It used to open a
        // modal that instantiated Monaco - a ~4MB chunk, its workers and a full
        // code editor - to show what is usually a few hundred bytes of object,
        // and the dialog then sat over the rows the value came from. The dock
        // renders it with `JsonTree`, follows the cell cursor, and costs nothing
        // to open.
        if (isJson) { e.stopPropagation(); openCellEditor(idx, actualIdx); return }

        // Forward FK: Ctrl/Cmd = full navigation; plain click = inline sub-view
        const fk = cached?.fk ?? null
        if (fk && !isNull) {
          if (e.metaKey || e.ctrlKey) {
            tryFollowForeignKey(idx, actualIdx, e)
            return
          }
          const fkLbl = foreignKeyTargetLabel(fk)
          if (fkSubview?.rowIdx === idx && fkSubview?.kind === 'forward' && fkSubview?.label === fkLbl) {
            fkSubview = null
            return
          }
          // Opening FK sub-view: close JSON expand for the same row (mutually exclusive)
          if (expandedRows.has(idx)) { const s = new Set(expandedRows); s.delete(idx); expandedRows = s }
          fkSubview = { rowIdx: idx, kind: 'forward', label: fkLbl, colIdx: actualIdx, data: { loading: true, columns: [], rows: [], error: null } }
          scrollRowIntoViewBesideDock(idx)
          void onfetchrelatedrows({ kind: 'forward', fk, row: rows[idx] ?? [] }).then(res => {
            if (fkSubview?.rowIdx !== idx || fkSubview?.label !== fkLbl) return
            fkSubview = { ...fkSubview, data: { loading: false, columns: res.columns ?? [], rows: res.rows ?? [], error: res.error ?? null } }
          })
          tableContainer?.focus({ preventScroll: true })
          return
        }

        // URL cell → open like the old anchor.
        if (!fk) {
          const href = cellLinkHref(formatCell(value))
          if (href) {
            const ut = cellUrlType(href, t.col.name)
            const isMedia = ut === 'image' || ut === 'pdf'
            // The lightbox is gated by the PREVIEW setting, not the URL setting:
            // it opens the file inside the app, so it is a preview, not "leaving
            // to follow a link". That split is what lets someone keep previews
            // while refusing to hand clicks to the browser, and vice versa.
            if (isMedia && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
              if ($appImagePreview) { lightboxUrl = href; lightboxType = /** @type {'image'|'pdf'} */ (ut); return }
              if ($appOpenUrlsOnClick) { void openExternal(href); return }
              tableContainer?.focus({ preventScroll: true })
              return
            }
            if ($appOpenUrlsOnClick) { void openExternal(href); return }
            tableContainer?.focus({ preventScroll: true })
            return
          }
          // Linkifier extension → deep-link an ID to an external system. Same
          // gate: it is still a click that leaves the app.
          if (!isNull && $appOpenUrlsOnClick) {
            const linked = linkifyValue(value, cached?.colType ?? '', t.col.name)
            if (linked) { void openExternal(linked); return }
          }
        }
        tableContainer?.focus({ preventScroll: true })
        return
      }
    }
  }

  function onCanvasDblClick(/** @type {MouseEvent} */ e) {
    _focusFromKey = false
    const { x, y } = canvasXY(e)
    // A relationship cell has no edit behind it, so the second click is free to
    // mean what it means on a foreign key: go there. Same destination as the
    // dock's "Open in sub view" - the related rows as a table of their own.
    const relHit = vrelHitAt(x, y)
    if (relHit) {
      if (relHit.rowIdx >= 0) openReverseFkFullView(relHit.rowIdx, relHit.vc)
      return
    }
    const t = hitTest(x, y)
    if (t.kind !== 'cell') return
    const idx = /** @type {number} */ (t.idx)
    const actualIdx = /** @type {number} */ (t.actualIdx)
    if (tryFollowForeignKey(idx, actualIdx, e)) return
    startEdit(idx, actualIdx)
  }

  function onCanvasAuxClick(/** @type {MouseEvent} */ e) {
    if (e.button !== 1) return
    _focusFromKey = false
    const { x, y } = canvasXY(e)
    const t = hitTest(x, y)
    if (t.kind === 'cell') tryFollowForeignKey(/** @type {number} */ (t.idx), /** @type {number} */ (t.actualIdx), e)
  }

  /** Swallow pinch/scroll zoom on resize handles; reset stray webview page-zoom. */
  function blockPointerZoom(/** @type {Event} */ e) {
    e.preventDefault()
    e.stopPropagation()
    void import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) => getCurrentWebview().setZoom(1))
      .catch(() => {})
  }

  /** @param {PointerEvent} e @param {string} colName */
  function onResizeHandleDown(e, colName) {
    if (e.button !== 0) return
    e.preventDefault()
    e.stopPropagation()
    startColumnResize(colName)
    clearActiveResizeListeners()
    const startX = e.clientX
    const move = (/** @type {PointerEvent} */ ev) => applyColumnResize(ev.clientX - startX)
    const up = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      _activeResizeListeners = null
      endColumnResize()
      _suppressNextClick = true
      setTimeout(() => { _suppressNextClick = false }, 0)
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    _activeResizeListeners = { move, up }
  }

  /** @param {string} colName */
  function onResizeHandleEnter(colName) {
    _resizeHoverCol = colName
    void import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) => getCurrentWebview().setZoom(1))
      .catch(() => {})
  }

  function onResizeHandleLeave() {
    if (!resizingColName) {
      _resizeHoverCol = null
    }
  }

  function onCanvasPointerMove(/** @type {PointerEvent} */ e) {
    const { x, y } = canvasXY(e)
    if (resizingColName) return
    if (y < HEADER_H) {
      _resizeHoverCol = null
      const cx = x + _scrollLeft
      const hit = cx >= gutterWidth ? colAtX(x, geom, _scrollLeft, 0) : null
      hoveredRow = null
      hoveredColName = hit ? hit.col.name : null
      return
    }
    _resizeHoverCol = null
    // Check virtual expr columns (between real cols and vrel cols)
    if (y >= HEADER_H && _vexprLayout.length > 0) {
      const cx = x + _scrollLeft
      for (const vc of _vexprLayout) {
        if (cx >= vc.x && cx < vc.x + vc.w) {
          const bodyY = y + _scrollTop - HEADER_H - insertRowOffset
          const r = rowAtContentY(rowTops, rows.length, ROW_HEIGHT, bodyY)
          if (r?.inRowBody) { hoveredRow = r.idx; hoveredColName = vc.hoverKey }
          return
        }
      }
    }
    // Check virtual rel columns (they are to the right of real + expr columns)
    if (y >= HEADER_H && virtualRelCols.length > 0) {
      const cx = x + _scrollLeft
      const vi = _vrelLayout.findIndex(vp => cx >= vp.x && cx < vp.x + vp.w)
      if (vi >= 0) {
        const bodyY = y + _scrollTop - HEADER_H - insertRowOffset
        const r = rowAtContentY(rowTops, rows.length, ROW_HEIGHT, bodyY)
        if (r?.inRowBody) { hoveredRow = r.idx; hoveredColName = _vrelHoverKeys[vi] }
        return
      }
    }
    const t = hitTest(x, y)
    if (t.kind === 'cell') {
      hoveredRow = /** @type {number} */ (t.idx)
      hoveredColName = t.col.name
    } else {
      hoveredRow = t.kind === 'row-expand' || t.kind === 'row-select' ? /** @type {number} */ (t.idx ?? null) : null
      hoveredColName = null
    }
  }

  function onCanvasPointerLeave() {
    hoveredRow = null
    hoveredColName = null
    _resizeHoverCol = null
  }

  // Re-sync canvas backing store when devicePixelRatio changes (stray webview
  // page-zoom on macOS). Without this the CSS box and bitmap can drift → blur.
  $effect(() => {
    if (!tableContainer) return
    let lastDpr = window.devicePixelRatio
    const resync = () => {
      invalidateCanvasRect() // window/zoom change moves the canvas - refresh on next hit-test
      const dpr = window.devicePixelRatio
      if (dpr === lastDpr) return
      lastDpr = dpr
      if (syncCanvasSurface()) scheduleDraw()
    }
    window.addEventListener('resize', resync)
    window.visualViewport?.addEventListener('resize', resync)
    return () => {
      window.removeEventListener('resize', resync)
      window.visualViewport?.removeEventListener('resize', resync)
    }
  })

  /** Right-click: record the target so the single ContextMenu shows the right items. */
  function onCanvasContextMenu(/** @type {MouseEvent} */ e, /** @type {((e: MouseEvent) => void) | undefined} */ bitsOpen) {
    const { x, y } = canvasXY(e)
    // Check virtual expr column header right-click (not caught by hitTest)
    if (y < HEADER_H && _vexprLayout.length > 0) {
      const cx = x + _scrollLeft
      for (const vc of _vexprLayout) {
        if (cx >= vc.x && cx < vc.x + vc.w) {
          contextIsHeader = true
          contextHeaderCol = vc.hoverKey
          bitsOpen?.(e)
          return
        }
      }
    }
    const t = hitTest(x, y)
    if (t.kind === 'header') {
      contextIsHeader = true
      contextHeaderCol = t.col.name
      bitsOpen?.(e)
      return
    }
    if (t.kind === 'cell' || t.kind === 'row-select' || t.kind === 'row-expand') {
      contextIsHeader = false
      contextRowIdx = /** @type {number} */ (t.idx)
      contextColIdx = t.kind === 'cell' ? /** @type {number} */ (t.actualIdx) : 0
      bitsOpen?.(e)
      return
    }
    e.preventDefault()
  }

  // Editing-overlay geometry (viewport-relative position of the active cell).
  const editOverlay = $derived.by(() => {
    if (!editingCell) return null
    const col = geom.cols.find((c) => _nameToActualIdx.get(c.name) === editingCell.colIdx)
    if (!col) return null
    // VIEWPORT coordinates - the editor lives in a sticky viewport layer, not the
    // scroll sizer. On a huge (windowed / scaled) table the sizer's content-space
    // y reaches tens of millions of px, past WebKit's layout range, so a DOM
    // element positioned there silently fails to paint. rowViewportY / colDrawnX
    // stay in the 0…viewport range and work identically in normal and scaled modes.
    const top = rowViewportY(editingCell.rowIdx)
    const left = colDrawnX(col, geom, _scrollLeft)
    return { top, left, width: col.w, height: ROW_HEIGHT }
  })

  onDestroy(() => {
    // Clear staged-edit state in the parent so the StatusBar buttons don't linger.
    pendingEditCount = 0
    applyEdits = () => {}
    copyEditsSql = () => {}
    resetEdits = () => {}
    scrollToTop = () => {}
    scrollToBottom = () => {}
    beginInsertRow = () => {}
    // Reset every other parent-bound closure too - StudioShell keeps these in
    // its own $state, so a stale one would retain this destroyed instance's
    // whole scope (display caches, expandedRows, FK sub-view rows).
    scrollToLeft = () => {}
    scrollToRight = () => {}
    focusColumn = () => {}
    focusCell = () => {}
    getScroll = () => ({ left: 0, top: 0 })
    getExpanded = () => []
    applyScroll = () => {}
    stageDeleteSelected = () => {}
  })
</script>

<div class="flex min-h-0 flex-1 overflow-hidden">
<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <ContextMenu.Root
    onOpenChange={(open) => {
      contextMenuOpen = open;
      if (open) {
        armMenuSelectGuard();
      } else {
        suppressMenuSelect = false;
      }
    }}
  >
    <ContextMenu.Trigger>
      {#snippet child({ props })}
        {@const bitsContextMenu = props.oncontextmenu}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div
          bind:this={tableContainer}
          data-canvas-table=""
          {...props}
          tabindex={-1}
          role="grid"
          aria-label={`${tableName || 'Data'} table, ${rows.length} ${rows.length === 1 ? 'row' : 'rows'}`}
          aria-rowcount={rows.length}
          aria-colcount={navigableColumns.length}
          class={cn(
            "app-scroll relative overflow-auto bg-panel select-none outline-none [scrollbar-gutter:stable] [contain:layout] [overflow-anchor:none]",
            embedded ? "max-h-80" : "min-h-0 flex-1",
            embedded && loading && columns.length === 0 && "min-h-40",
            resizingColName && "cursor-col-resize",
          )}
          oncontextmenu={(e) => onCanvasContextMenu(e, bitsContextMenu)}
          onscroll={onContainerScroll}
          onkeydown={handleTableKeydown}
          onpointerdown={() => {
            // Focus arrives from the pointer later in this same task, so the
            // reset lands after the focusin it causes.
            _focusFromPointer = true;
            setTimeout(() => { _focusFromPointer = false; }, 0);
          }}
          onfocusin={(e) => {
            isTableFocused = true;
            // Tab in from the sidebar lands on the grid itself. With no cell
            // cursor there is nothing on screen to say focus arrived at all, so
            // the first cell takes it - the same one an arrow key would have
            // moved to. Only when the container is the target: focus entering a
            // cell editor or a new-row field must not reset the cursor.
            //
            // Not on a click, though. A click already names the cell it means,
            // and it sets the cursor a moment later - seeding 0,0 on the way in
            // drew the ring on the first cell for a frame and then moved it,
            // which is the jump you see on the first click into a fresh table.
            if (
              e.target === tableContainer && !_focusFromPointer &&
              focusedRow === null && rows.length && visibleColumns.length
            ) {
              focusedRow = 0; focusedCol = 0;
            }
          }}
          onfocusout={(e) => {
            if (!tableContainer?.contains(e.relatedTarget instanceof Element ? e.relatedTarget : null)) {
              isTableFocused = false;
            }
          }}
        >
          <!-- Hidden probe: resolves theme CSS custom properties to concrete
               rgb() colours the canvas 2D context can use. -->
          <span
            bind:this={colorProbe}
            aria-hidden="true"
            class="font-mono"
            style="position:absolute;top:0;left:0;width:0;height:0;overflow:hidden;pointer-events:none"
          ></span>

          <!-- Screen-reader announcement for the focused cell. Visually hidden;
               updated only when focus/edit state changes (not during scroll). -->
          <div
            class="sr-only"
            role="status"
            aria-live="polite"
            aria-atomic="true"
          >{a11yCellAnnouncement}</div>


          <!-- Canvas: always mounted so the 2D context survives table navigation
               (each mount creates a new GPU-tracked context; keeping it alive
               across table switches eliminates the accumulation shown in DevTools). -->
          <!-- 0x0 sticky anchor that pins the canvas to the top-left of the
               scrollport. No `will-change: transform` here: the box never moves
               under its own power, and promoting an empty sticky box to its own
               compositor layer is what let WebKit keep painting it at a stale
               offset while the scroll container was being RESIZED - the grid
               appeared to slide up and down as the related-rows dock was dragged.
               The canvas inside keeps its own will-change; that one is real. -->
          <div style="position:sticky;top:0;left:0;width:0;height:0;z-index:1;overflow:visible">
            <canvas
              bind:this={canvasEl}
              class="block"
              style="cursor:default;will-change:transform"
              onclick={guarded(onCanvasClick)}
              ondblclick={guarded(onCanvasDblClick)}
              onauxclick={guarded(onCanvasAuxClick)}
              onpointermove={guarded(onCanvasPointerMove)}
              onpointerleave={guarded(onCanvasPointerLeave)}
            ></canvas>
            <!-- Inline insert-row form, inside the canvas's own sticky
                 anchor. The staged rows have to land on the columns the canvas
                 draws, so they take their origin from the same pinned box the
                 canvas does rather than a second one of their own: whatever the
                 viewport's left edge turns out to be, both are measuring from
                 it. In the sizer instead, a sticky box has to satisfy its `left`
                 inset and its containing block's right edge at once, and which
                 of those wins depends on how wide the box is - which is a
                 property of the table, not of the band. -->
            {#if newRowDrafts?.length}
            {#each newRowDrafts as rowDraft, di (di)}
              <!-- `cell-fields`: the app-wide rule (app.css) that makes a field
                   inside a grid cell flush - no border, no radius, no background
                   of its own. Without it every draft input drew the 2px border at
                   the 12px field radius the unlayered bare-input rule gives any
                   input, so a row of 28px cells came out as a row of pills inside
                   a row that already has its own rules and its own insert ring.
                   The same fix the cell editor and the structure grid carry.
                   -
                   One band per staged row, stacked under the header in the order
                   they were added. `top` is a viewport y, so the stack holds its
                   place under the header for nothing per scroll frame, and
                   `z-index` descends so an earlier row's ring is never drawn
                   over by the one below it.
                   -
                   Horizontally the box is the viewport's width and clips; only
                   the layer inside it moves, translated by the same
                   `_scrollLeft` the canvas draws with. Left to native scroll it
                   moved the instant the wheel did while the columns behind it
                   repainted on the next frame, and the two slid past each other
                   - the parallax you see dragging a wide table sideways. -->
              <div
                role="none"
                data-new-row={di}
                class={cn(
                  // One ring per row drew a line between every pair of staged
                  // rows on top of the border that was already there, which is
                  // the doubled edge. The stack reads as a band instead: a faint
                  // tint throughout, ordinary row rules between, and one firm
                  // edge where it meets the data.
                  // The pickers carry their own type scale, so the size is
                  // pushed onto everything the band renders - a staged row has
                  // to line up with the rows under it, and those are drawn at
                  // the grid's own font size, not at a rung of the UI scale.
                  'cell-fields absolute overflow-hidden [&_button]:text-[length:inherit] [&_input]:text-[length:inherit] [&_span]:text-[length:inherit]',
                )}
                style="top:{HEADER_H + di * ROW_HEIGHT}px; left:0; height:{ROW_HEIGHT}px; width:{_viewportWidth}px; z-index:{20 - Math.min(di, 9)}; font-size:{gridMetrics.cellPx}px"
                onkeydown={(e) => onNewRowKeydown(e, di)}
              >
                <!-- The scrolling layer. The box above is pinned to the
                     viewport and clips; this is the only thing that moves, and
                     it moves by the same `_scrollLeft` the canvas draws with,
                     in one transform for the whole row. Every cell inside is
                     placed at its column's content x, so the band cannot drift
                     away from the grid however far sideways you drag. -->
                <div
                  class={cn(
                    'absolute inset-y-0 left-0 bg-panel',
                    di === draftCount - 1
                      ? 'border-b-2 border-success/35'
                      : 'border-b border-border/30',
                  )}
                  style="width:{totalContentWidth}px; transform:translateX({-_scrollLeft}px)"
                >
                {#if showRowExpand}
                  <div class="absolute inset-y-0 flex items-center justify-center border-r border-border/20 bg-primary/5" style="left:0; width:{GUTTER_EXPAND_W}px">
                    {#if insertSaving}
                      <Loader class="size-3 animate-spin text-muted-foreground" />
                    {:else}
                      <!-- The tick on the FIRST row inserts every staged row;
                           on the rest it inserts just that one. One click for
                           the batch is what you want after filling several in,
                           and the row you are looking at is what you want when
                           only one of them is ready. -->
                      {@const missingHere = insertMissing[di] ?? []}
                      <Check
                        class={cn(
                          'size-3 cursor-pointer',
                          missingHere.length ? 'text-warning' : 'text-primary',
                        )}
                        onclick={() => void submitNewRow(di === 0 ? null : di)}
                        title={missingHere.length
                          ? `${missingHere.length} required field${missingHere.length === 1 ? '' : 's'} still empty: ${missingHere.join(', ')}`
                          : di === 0 && draftCount > 1
                            ? `Insert all ${draftCount} rows (⌘↵)`
                            : 'Insert this row (⌘↵)'}
                      />
                    {/if}
                  </div>
                {/if}
                {#if showSelection}
                  <div class="absolute inset-y-0 flex items-center justify-center border-r border-border/20 bg-primary/5" style="left:{showRowExpand ? GUTTER_EXPAND_W : 0}px; width:{GUTTER_SELECT_W}px">
                    <button
                      type="button"
                      class="inline-flex size-4 items-center justify-center rounded text-muted-foreground hover:text-destructive"
                      onclick={() => removeDraftRow(di)}
                      title={draftCount > 1 ? 'Discard this row' : 'Cancel'}
                    >
                      <X class="size-3" />
                    </button>
                  </div>
                {/if}
                <!-- The row-number gutter. Nothing to number in a row that
                     does not exist yet, so it draws as an empty slot - but it
                     has to BE there, because the canvas puts every column
                     after it and the band has to agree on where the columns
                     start. -->
                {#if GUTTER_NUM_W > 0}
                  <div
                    class="absolute inset-y-0 border-r border-border/20 bg-primary/5"
                    style="left:{(showRowExpand ? GUTTER_EXPAND_W : 0) + (showSelection ? GUTTER_SELECT_W : 0)}px; width:{GUTTER_NUM_W}px"
                    aria-hidden="true"
                  ></div>
                {/if}
                {#each visibleColumns as col, ci (col.name)}
                  {@const gcol = geom.cols[ci]}
                  {@const dt = col.dataType ?? col.data_type ?? ''}
                  {@const omit = insertOmitBehaviour(col, primaryKey)}
                  {@const isAuto = omit === 'auto'}
                  {@const blankLabel = omit === 'default' ? 'default' : omit === 'null' ? 'NULL' : 'Required'}
                  <!-- Only a Required blank stops the insert, so only it is
                       worth noticing before you submit. The rest describe a
                       value the database will supply and recede accordingly -
                       nothing is wrong yet, so nothing is coloured as wrong. -->
                  {@const blankClass = omit === 'required'
                    ? 'placeholder:text-muted-foreground'
                    : 'placeholder:italic placeholder:text-muted-foreground'}
                  {@const enumValues = getColumnEnumValues(col)}
                  {@const isBoolean = isBooleanType(dt)}
                  {@const isDateTime = shouldUseDateTimePicker(dt, col.name)}
                  {@const isDateOnly = isDateOnlyType(dt)}
                  {@const isTimeOnly = isTimeOnlyType(dt)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  {#if gcol}
                  <div
                    class="absolute inset-y-0 flex items-center overflow-hidden border-r border-border/35 bg-success/[0.03] px-2"
                    style="left:{bandCellX(gcol)}px; width:{gcol.w}px;{gcol.pinned ? ' z-index:1;' : ''}"
                    oncontextmenu={(e) => openDraftMenu(e, di, col.name)}
                  >
                    {#if isAuto}
                      <!-- Writable, with the generated value as the placeholder.
                           Leaving it blank is the normal path and the label says
                           so; typing an explicit id is legitimate (importing a
                           row that must keep its key, backfilling a gap) and
                           refusing it means dropping to raw SQL for a one-cell
                           exception. Empty still omits the column entirely, so
                           the sequence is untouched unless you overrule it. -->
                      <KeyRound class="mr-1 size-3 shrink-0 text-muted-foreground" />
                      <input
                        data-new-row-input={col.name}
                        type="text"
                        disabled={insertSaving}
                        placeholder={dt.toLowerCase().includes('int') || dt.toLowerCase().includes('serial')
                          ? 'auto-increment'
                          : 'generated'}
                        title="The database fills this in. Type a value only to override it."
                        class="w-full min-w-0 bg-transparent font-mono text-[length:inherit] text-foreground outline-none placeholder:italic placeholder:text-muted-foreground disabled:opacity-50"
                        value={rowDraft[col.name] ?? ''}
                        oninput={(e) => setNewRowDraft(di, col.name, e.currentTarget.value)}
                        onfocus={() => { newRowFocusCol = col.name; newRowFocusIdx = di }}
                      />
                    {:else if enumValues || isBoolean}
                      <!-- The same searchable menu the inline cell editor uses
                           for an enum, so picking a value is one control in the
                           app rather than two takes on it. The blank row stays
                           first and says what leaving the field alone does -
                           `default`, `NULL` or `Required` - which is the one
                           thing an insert needs that an edit never does. -->
                      {@const opts = enumValues ?? ['true', 'false']}
                      {@const picked = rowDraft[col.name] ?? ''}
                      <SearchableMenu
                        items={[{ value: '', label: blankLabel }, ...opts.map((o) => ({ value: o, label: o }))]}
                        placeholder="Search values…"
                        contentClass="w-56"
                        align="start"
                        onselect={(it) => setNewRowDraft(di, col.name, it.value ?? '')}
                      >
                        {#snippet trigger(props)}
                          <button
                            {...props}
                            data-new-row-input={col.name}
                            type="button"
                            disabled={insertSaving}
                            aria-label="{col.name} value"
                            class="flex h-full w-full min-w-0 items-center gap-1 bg-transparent text-left font-mono text-[length:inherit] outline-none disabled:opacity-50"
                            onfocus={() => { newRowFocusCol = col.name; newRowFocusIdx = di }}
                          >
                            <span class={cn('min-w-0 flex-1 truncate', picked ? 'text-foreground' : blankClass.replace(/placeholder:/g, ''))}>
                              {picked || blankLabel}
                            </span>
                            <Icon name="chevron-down" class="size-3 shrink-0 opacity-50" />
                          </button>
                        {/snippet}
                        {#snippet item(it)}
                          <span class="min-w-0 flex-1 truncate">{it.label}</span>
                          {#if picked === it.value}
                            <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                          {/if}
                        {/snippet}
                      </SearchableMenu>
                    {:else if isDateTime || isDateOnly}
                      <!-- The same shape the inline cell editor uses: calendar
                           on the trailing edge, value at the grid's type size,
                           field filling the cell. It wore the picker's default
                           layout - icon first, its own font - so a staged row
                           looked like a form dropped on top of the table rather
                           than a row of it. -->
                      <DateTimePicker
                        colName={col.name}
                        showTime={isDateTime}
                        disabled={insertSaving}
                        iconTrailing={true}
                        class="h-full w-full min-w-0 pr-1"
                        inputClass="text-[length:inherit]"
                        value={rowDraft[col.name] ?? ''}
                        onchange={(v) => setNewRowDraft(di, col.name, v)}
                        onfocus={() => { newRowFocusCol = col.name; newRowFocusIdx = di }}
                      />
                    {:else if isTimeOnly}
                      <input
                        data-new-row-input={col.name}
                        type="time"
                        disabled={insertSaving}
                        class="w-full bg-transparent font-mono text-[length:inherit] text-foreground outline-none disabled:opacity-50"
                        value={rowDraft[col.name] ?? ''}
                        oninput={(e) => setNewRowDraft(di, col.name, e.currentTarget.value)}
                        onfocus={() => { newRowFocusCol = col.name; newRowFocusIdx = di }}
                      />
                    {:else}
                      <input
                        data-new-row-input={col.name}
                        type="text"
                        disabled={insertSaving}
                        placeholder={blankLabel}
                        class={cn(
                          "w-full bg-transparent font-mono text-[length:inherit] text-foreground outline-none disabled:opacity-50",
                          blankClass,
                        )}
                        value={rowDraft[col.name] ?? ''}
                        oninput={(e) => setNewRowDraft(di, col.name, e.currentTarget.value)}
                        onfocus={() => { newRowFocusCol = col.name; newRowFocusIdx = di }}
                      />
                    {/if}
                  </div>
                  {/if}
                {/each}
                </div>
              </div>
            {/each}
            {/if}
          </div>

          {#if visibleColumns.length > 0}
            <!-- Column resize handles: DOM overlay so header edge interaction never
                 hits the canvas (macOS trackpad pinch on canvas was page-zooming
                 the webview and making the grid look huge/blurry).
                 Mirrors the canvas sticky wrapper (sticky top+left 0, width:0,
                 overflow:visible) so handles stay in viewport coordinates even
                 when the table is scrolled horizontally, without left:0 the
                 wrapper drifts with the content and every handle shifts by scrollLeft. -->
            <div
              style="position:sticky;top:0;left:0;width:0;height:0;z-index:2;overflow:visible;pointer-events:none"
              aria-hidden="true"
            >
              {#each resizeHandles as h (h.name)}
                <div
                  role="separator"
                  aria-orientation="vertical"
                  aria-label="Resize {h.name.startsWith('__vrel__') ? 'relationship column' : `column ${h.name}`}"
                  class="absolute top-0 w-2.5 -translate-x-1/2 cursor-col-resize pointer-events-auto touch-none"
                  style="left:{h.x}px; height:{HEADER_H}px"
                  onpointerdown={(e) => onResizeHandleDown(e, h.name)}
                  onpointerenter={() => onResizeHandleEnter(h.name)}
                  onpointerleave={onResizeHandleLeave}
                  onwheel={blockPointerZoom}
                  onmousewheel={blockPointerZoom}
                  ongesturestart={blockPointerZoom}
                  ongesturechange={blockPointerZoom}
                  ongestureend={blockPointerZoom}
                ></div>
              {/each}
            </div>


            <!-- Sizer: establishes the scroll range; DOM overlays are positioned
                 within it in content coordinates. -->
            <div
              class="relative"
              style="width:{resizingColName ? Math.max(totalContentWidth, _scrollLeft + _viewportWidth) : totalContentWidth}px; height:{spacerHeight}px"
            >
              <!-- JSON expand panels (independent from FK sub-view).
                   Same pin pattern as the FK sub-view: outer absolute for vertical
                   position, inner position:sticky;left:0 for the horizontal pin. This
                   lets the compositor hold it at the viewport-left edge during
                   horizontal scroll instead of a reactive transform:translateX() that
                   lags a frame behind the native scroll (the "vibration"). -->
              {#snippet expandBody(/** @type {number} */ exIdx)}
                <!-- `rows` is $state.raw and a saved edit swaps one row in place, so
                     the record reads `dataVersion` to rebuild; staged edits come
                     through effectiveCellValue, the same values the grid paints. -->
                <RowExpandViewer
                  record={expandedRecord(exIdx, dataVersion)}
                  rowLabel={"row " + (exIdx + 1)}
                  indent={gutterWidth}
                  onclose={() => toggleRowExpand(exIdx)}
                  onopenjson={(value, label) => openValueInDock(value, label)}
                />
              {/snippet}
              {#each [...expandedRows] as exIdx (exIdx)}
                {#if rows[exIdx] !== undefined}
                  <!-- Keeping the panel out of the header band is geometry, not a
                       clip. The panel has to sit above the canvas (the canvas paints
                       an opaque background), so once its row scrolls up behind the
                       header it covers the column headers, the select-all checkbox
                       and the collapse-all chevron - all of which are canvas-painted
                       and hit-tested by coordinate, so a DOM box on top of them
                       swallows the click before the canvas handler ever runs.
                       `clip-path` hid that box but did not stop it from taking the
                       pointer, which is why the header stayed visible and went dead.

                       So the box itself stops at the band: the top edge moves down by
                       the overlap, `overflow:hidden` trims what is now outside, and a
                       matching negative margin holds the content at its true position,
                       so nothing appears to move.

                       The trim is applied only while there is something to trim.
                       The scroll container carries `contain:layout`, which makes it
                       the containing block for the panel's `position:fixed` context
                       menu, so leaving `overflow:hidden` on permanently would clip
                       that menu.

                       `trackExpandHeight` measures the inner element, whose
                       offsetHeight is the real panel height. Measuring the trimmed
                       box instead would feed a shrinking height into rowTops and
                       make every row below jitter while scrolling. Only panels
                       actually touching the band pay anything, and only while they
                       do. -->
                  <!-- Every table: a viewport-pinned layer, placed at the same scroll
                       position the canvas paints with. It was content space on normal
                       tables, moved by the native scroll; WebKit moves that on its own
                       scrolling thread, ahead of the canvas, so the rows under a panel
                       slid against it while scrolling (measured: the gap below one
                       changed from 18 to 35px across consecutive frames). Pinned, the
                       panel and the rows move in the same frame.

                       Moved by a transform, never by `top`: a `top` write lays the
                       panel out again on every scroll frame. No `will-change` on
                       it: a promoted layer here painted at a stale offset for a
                       frame on tab switches and resizes (see the canvas anchor). -->
                  {#if rowViewportY(exIdx) > -_viewportHeight * 2 && rowViewportY(exIdx) < _viewportHeight + ROW_HEIGHT}
                    <!-- Only near the viewport: on a huge table the y would be past
                         WebKit's layout range, and the height a panel had is kept while
                         it is away (trackExpandHeight), so the rows below do not move. -->
                    {@const clipTopScaled = Math.max(0, HEADER_H - (rowViewportY(exIdx) + ROW_HEIGHT))}
                    <div style="position:sticky;top:0;left:0;width:0;height:0;overflow:visible;z-index:10">
                      <div
                        class="absolute left-0 top-0"
                        style="transform:translateY({rowViewportY(exIdx) + ROW_HEIGHT + clipTopScaled}px); width:{_viewportWidth}px{clipTopScaled > 0 ? '; overflow:hidden' : ''}"
                      >
                        <div style={clipTopScaled > 0 ? `margin-top:-${clipTopScaled}px` : ''} use:trackExpandHeight={exIdx}>
                          {@render expandBody(exIdx)}
                        </div>
                      </div>
                    </div>
                  {/if}
                {/if}
              {/each}

              <!-- FK sub-view renders in the docked bottom panel (after the scroll
                   container), never inline between rows, so grid scrolling stays
                   clean and the canvas hot path is untouched. -->

              <!-- Active inline cell editor. Rendered in a sticky viewport layer
                   (not the scroll sizer) so its top/left stay in the 0…viewport
                   range, a DOM node at the sizer's multi-million-px content y
                   silently fails to paint in WebKit on huge / scaled tables. -->
              <div style="position:sticky;top:0;left:0;width:0;height:0;overflow:visible;z-index:30">
              {#if editingCell && editOverlay}
                <!-- Every read below goes through `?.` even though the {#if} already
                     proved the cell is non-null. Committing an edit (date pick, enum
                     pick, boolean toggle) sets `editingCell = null` synchronously, and
                     these {@const}s are deriveds that a child's lazy prop getter can
                     force to revalidate before the {#if} tears this branch down -
                     `columns[editingCell.colIdx]` then throws "null is not an object".
                     Editing a `created_at`-style column reproduced exactly that. -->
                {@const ecol = columns[editingCell?.colIdx ?? -1]}
                {@const ecached = _colCache[editingCell?.colIdx ?? -1]}
                {@const eEnum = ecached?.enumValues ?? null}
                {@const eType = ecached?.colType ?? ''}
                {@const eNullable = ecol?.nullable ?? true}
                {@const eIsArray = isPgArrayDialect && isSqlArrayType(eType)}
                {@const eIsJson = !eIsArray && /json/i.test(eType)}
                {@const eDateTime = !eIsArray && !eIsJson && shouldUseDateTimePicker(eType, ecol?.name ?? '')}
                {@const eDateOnly = !eIsArray && !eIsJson && isDateOnlyType(eType)}
                {@const eTimeOnly = !eIsArray && !eIsJson && isTimeOnlyType(eType)}
                <!-- The editor has to land on the glyphs it replaces. The canvas
                     draws cell text at CELL_PAD_X and flips to flush-right per the
                     Appearance setting; an editor that is always left-aligned at
                     px-3 made the value jump sides and shift 2px the moment you
                     started typing. Same padding, same alignment, no jump. -->
                {@const eAlignRight = isRightAlignedColumn(editingCell?.colIdx ?? -1)}
                <!-- The size the CANVAS drew this value at, not a `text-ui-*` rung.
                     The two are different numbers: the grid's text is the user's
                     grid text size scaled by `canvasZoom`, the UI scale is a
                     separately-rounded ladder off the root px. The fields were on
                     the second one, so a value grew a pixel or two and reflowed
                     the moment you started editing it - the same jump the padding
                     comment above describes, in the other axis, and one that
                     showed up on every platform because nothing about it was
                     platform-specific. -->
                {@const eFontStyle = `font-size:${gridMetrics.cellPx}px`}
                {@const eFieldStyle = `padding-left:${CELL_PAD_X}px;padding-right:${CELL_PAD_X}px;text-align:${eAlignRight ? 'right' : 'left'};${eFontStyle}`}
                <!-- The date editor carries a calendar button beside its field, so
                     it pads only the side the text is anchored to and lets the
                     button sit on the other one. Padding both sides would push the
                     value off the x the canvas drew it at by the button's width. -->
                {@const ePickerStyle = eAlignRight
                  ? `padding-right:${CELL_PAD_X}px;text-align:right;${eFontStyle}`
                  : `padding-left:${CELL_PAD_X}px;text-align:left;${eFontStyle}`}
                <div
                  in:fade={{ duration: 100, easing: cubicOut }}
                  data-cell-editor
                  class="absolute z-30 box-border bg-background ring-2 ring-inset ring-primary"
                  style="top:{editOverlay.top}px; left:{editOverlay.left}px; width:{editOverlay.width}px; height:{editOverlay.height}px"
                >
                  {#if eEnum}
                    <!-- SearchableMenu directly, configured exactly like the
                         filter-condition menu in TableToolbar - same primitive,
                         same fixed content width, same trigger/item snippets - so
                         the two read as one control rather than two takes on it.
                         -
                         NOT anchored to the cell width: a narrow column would
                         squash the search field down to its icon. NOT font-mono
                         either; every other menu in the app is sans, and the
                         values are short labels, not data being compared
                         character by character.
                         -
                         Picking commits; closing without a pick cancels. -->
                    {@const enumItems = [
                      ...(eNullable ? [{ value: '', label: 'NULL' }] : []),
                      ...(editingCell?.original && !eEnum.includes(editingCell.original)
                        ? [{ value: editingCell.original, label: editingCell.original }]
                        : []),
                      ...eEnum.map((o) => ({ value: o, label: o })),
                    ]}
                    <SearchableMenu
                      bind:open={enumEditorOpen}
                      items={enumItems}
                      placeholder="Search values…"
                      contentClass="w-56"
                      align="start"
                      onselect={(it) => {
                        if (!editingCell) return
                        _enumPicked = true
                        editingCell.draft = it.value ?? ''
                        void commitEdit()
                      }}
                    >
                      {#snippet trigger(props)}
                        <button
                          {...props}
                          bind:this={editInput}
                          type="button"
                          aria-label="Edit {ecol?.name ?? 'cell'}"
                          class="flex h-full w-full min-w-0 items-center gap-1 text-left font-mono text-foreground outline-none"
                          style={eFieldStyle}
                        >
                          <span class="min-w-0 flex-1 truncate">
                            {editingCell?.draft || (eNullable ? 'NULL' : 'Select…')}
                          </span>
                          <Icon name="chevron-down" class="size-3 shrink-0 opacity-50" />
                        </button>
                      {/snippet}
                      {#snippet item(it)}
                        <span class="min-w-0 flex-1 truncate">{it.label}</span>
                        {#if (editingCell?.draft ?? '') === it.value}
                          <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                        {/if}
                      {/snippet}
                    </SearchableMenu>
                  {:else if isBooleanType(eType)}
                    {@const isOn = editingCell?.draft === "true"}
                    {@const isNull = eNullable && editingCell?.draft !== "true" && editingCell?.draft !== "false"}
                    <button
                      type="button"
                      bind:this={editInput}
                      disabled={saving}
                      aria-label="Toggle {ecol?.name ?? 'cell'}"
                      class="flex h-full w-full items-center gap-2.5 font-mono text-foreground outline-none"
                      style={eFieldStyle}
                      onclick={async (e) => {
                        e.stopPropagation();
                        editingCell.draft = editingCell.draft === "true" ? "false" : "true";
                        await commitEdit();
                      }}
                      onkeydown={handleEditKeydown}
                    >
                      <span class={cn("relative inline-flex h-4 w-7 shrink-0 items-center rounded-full transition-colors duration-150", isOn ? "bg-primary/80" : "bg-muted-foreground/30")}>
                        <span class="absolute size-3 rounded-full bg-white shadow-sm transition-transform duration-150" style={isOn ? "transform: translateX(14px)" : "transform: translateX(2px)"}></span>
                      </span>
                      <span class={isNull ? "text-muted-foreground" : ""}>{isNull ? "NULL" : editingCell?.draft === "true" ? "true" : "false"}</span>
                    </button>
                  {:else if eIsArray}
                    <!-- SQL array cell → dedicated array editor (add/remove/reorder),
                         reusing the same open/commit path as the context menu so the
                         edit stages via commitArrayEditor → stageEdit (Apply/undo work). -->
                    {@const eArrVal = editingCell ? effectiveCellValue(editingCell.rowIdx, editingCell.colIdx) : null}
                    <button
                      type="button"
                      bind:this={editInput}
                      disabled={saving}
                      aria-label="Edit array {ecol?.name ?? 'cell'}"
                      class="flex h-full w-full items-center gap-2 font-mono text-foreground outline-none"
                      style={eFieldStyle}
                      onclick={(e) => {
                        e.stopPropagation();
                        if (!editingCell) return;
                        const { rowIdx, colIdx } = editingCell;
                        cancelEdit();
                        openArrayEditor(rowIdx, colIdx);
                      }}
                      onkeydown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault(); e.stopPropagation();
                          if (!editingCell) return;
                          const { rowIdx, colIdx } = editingCell;
                          cancelEdit();
                          openArrayEditor(rowIdx, colIdx);
                        } else if (e.key === "Escape") {
                          e.preventDefault(); e.stopPropagation(); cancelEdit();
                        }
                      }}
                    >
                      <Braces class="size-3.5 shrink-0 text-muted-foreground" />
                      <span class="truncate">{Array.isArray(eArrVal) ? arrayDisplay(eArrVal) : (editingCell?.draft || "{}")}</span>
                      <Maximize2 class="ml-auto size-3 shrink-0 text-muted-foreground" />
                    </button>
                  {:else if eDateTime || eDateOnly}
                    <!-- Date/timestamp cell → calendar picker bound to the draft.
                         Picking commits immediately (like the enum dropdown): a
                         click-away cancels the inline edit, so we can't defer the
                         commit without losing the pick. -->
                    <!-- `editingCell?.draft`, not `editingCell.draft`: a prop is a
                         lazy getter, and picking a date runs onchange → commitEdit,
                         which sets `editingCell = null` synchronously. The picker's
                         own deriveds then re-read this getter before it unmounts, so
                         a bare dereference throws (null is not an object).
                         -
                         It wears the same padding, alignment and type size as the
                         plain text editor above, and hands its input to `editInput`
                         and its spare keys to `handleEditKeydown`, so a timestamp
                         commits on Enter and tabs on Tab like every other cell. -->
                    <DateTimePicker
                      bind:inputRef={editInput}
                      colName={ecol?.name}
                      showTime={eDateTime}
                      disabled={saving}
                      iconTrailing={!eAlignRight}
                      class={eAlignRight ? 'h-full pl-2' : 'h-full pr-2'}
                      inputClass="text-ui-xs"
                      inputStyle={ePickerStyle}
                      value={editingCell?.draft ?? ''}
                      oninput={(v) => {
                        // Typing only stages. Committing here would end the
                        // edit after the first character, which is what made
                        // the field look like it refused input.
                        if (editingCell) editingCell.draft = v;
                      }}
                      onchange={(v) => {
                        if (!editingCell) return;
                        editingCell.draft = v;
                        void commitEdit();
                      }}
                      onkeydown={handleEditKeydown}
                    />
                  {:else if eTimeOnly}
                    <input
                      bind:this={editInput}
                      type="time"
                      bind:value={editingCell.draft}
                      disabled={saving}
                      aria-label="Edit {ecol?.name ?? 'cell'}"
                      class="box-border block h-full w-full min-w-0 max-w-full border-0 bg-transparent font-mono text-foreground outline-none selection:bg-primary/20"
                      style={eFieldStyle}
                      onclick={(e) => e.stopPropagation()}
                      onkeydown={handleEditKeydown}
                    />
                  {:else}
                    <input
                      bind:this={editInput}
                      bind:value={editingCell.draft}
                      disabled={saving}
                      aria-label="Edit {ecol?.name ?? 'cell'}"
                      class="box-border block h-full w-full min-w-0 max-w-full overflow-x-auto border-0 bg-transparent font-mono text-foreground outline-none [field-sizing:fixed] selection:bg-primary/20"
                      style={eFieldStyle}
                      onclick={(e) => e.stopPropagation()}
                      onkeydown={handleEditKeydown}
                    />
                  {/if}
                </div>
              {/if}
              </div>
            </div>
          {/if}

          <!-- Empty states -->
          {#if loading && columns.length === 0}
            <!-- First load of a table. Drawn over the live grid rather than in
                 place of it: swapping the grid out destroyed the canvas on every
                 table open, and a load that landed a frame later flashed a
                 spinner plus a brand-new compositor layer. -->
            {#if loadingVisible}
              <div class="pointer-events-none absolute inset-0 z-[3] flex flex-col animate-in fade-in duration-300">
                <TableLoading {embedded} />
              </div>
            {/if}
          {:else if visibleColumns.length === 0}
            <div class="pointer-events-none absolute inset-0 z-[3] flex items-center justify-center" role="status" aria-live="polite">
              <div class="flex flex-col items-center gap-2 px-4 text-center">
                <Table2 class="size-8 text-muted-foreground" />
                <p class="text-ui-sm text-muted-foreground">No columns visible</p>
              </div>
            </div>
          {:else if rows.length === 0 && !newRowDrafts}
            <!-- "No rows" is a claim about the data, so it must never be shown
                 while a fetch is still running. The full-page skeleton above
                 only covers the first load (no columns yet); every later fetch
                 that empties the grid - a re-query, or the second half of a
                 windowed load - lands here, and without this it would assert
                 the table is empty for the whole of a multi-second read. -->
            <div class="pointer-events-none absolute inset-0 z-[3] flex items-center justify-center" role="status" aria-live="polite">
              <div class="flex flex-col items-center gap-2 px-4 text-center">
                {#if loading}
                  {#if loadingVisible}
                    <div class="flex flex-col items-center gap-2 animate-in fade-in duration-300">
                      <Loader class="size-5 animate-spin text-muted-foreground" />
                      <p class="text-ui-sm text-muted-foreground">Loading rows…</p>
                    </div>
                  {/if}
                {:else}
                  <Table2 class="size-8 text-muted-foreground" />
                  <p class="text-ui-sm text-muted-foreground">No rows in this table</p>
                {/if}
              </div>
            </div>
          {/if}

          <!-- Windowed mode: what the grid is waiting for.
               Pinned to the viewport (sticky in both axes, zero-size, so it adds
               nothing to the scroll extent) and only raised once the wait is long
               enough to be worth reading - a window that lands in 40ms never
               shows one. Says which rows, out of how many, and whether the fetch
               is merely slow or has actually given up. -->
          {#if windowed && loadingSpan}
            <div style="position:sticky;bottom:0;left:0;width:0;height:0;overflow:visible;z-index:6;pointer-events:none" aria-live="polite">
              <div style="position:absolute;bottom:14px;left:0;width:{_viewportWidth}px" class="flex justify-center">
                <div
                  class={cn(
                    "flex items-center gap-2 rounded-full border bg-background px-3 py-1 elevate-2-rim",
                    // Only the error state takes the pointer, for its Retry button.
                    // A pill that merely reports progress must not eat clicks on
                    // the rows it floats over.
                    windowStatus?.failed ? "pointer-events-auto border-destructive/30" : "border-border/25",
                  )}
                >
                  {#if windowStatus?.failed}
                    <TriangleAlert class="size-3 shrink-0 text-destructive" />
                    <span class="text-ui-2xs text-foreground/80">
                      Couldn't load rows {(loadingSpan.first + 1).toLocaleString()}-{(loadingSpan.last + 1).toLocaleString()}
                    </span>
                    <button
                      type="button"
                      class="-mr-1 flex h-5 items-center gap-1 rounded-full px-1.5 text-ui-2xs text-primary hover:bg-muted/40"
                      onclick={() => onretrywindows()}
                    >
                      <RotateCcw class="size-3 shrink-0" />
                      Retry
                    </button>
                  {:else}
                    <Loader class="size-3 shrink-0 animate-spin text-muted-foreground" />
                    <span class="text-ui-2xs text-muted-foreground">
                      Loading rows {(loadingSpan.first + 1).toLocaleString()}-{(loadingSpan.last + 1).toLocaleString()}
                      <span class="text-muted-foreground">of {rows.length.toLocaleString()}</span>
                    </span>
                    {#if windowStatus?.slow}
                      <span class="text-ui-2xs text-muted-foreground">· slow connection</span>
                    {/if}
                  {/if}
                </div>
              </div>
            </div>
          {/if}

          <!-- Infinite scroll: sticky bottom loading bar -->
          {#if infiniteScroll && loadingMore}
            <div
              style="position:sticky;bottom:0;left:0;width:100%;pointer-events:none;z-index:5"
              aria-live="polite"
              aria-label="Loading more rows"
            >
              <div class="flex items-center justify-center py-2">
                <div class="flex items-center gap-1.5 rounded-full border border-border/20 bg-background px-3 py-1 elevate-2-rim">
                  <Loader class="size-3 animate-spin text-muted-foreground" />
                  <span class="text-ui-2xs text-muted-foreground">Loading more…</span>
                </div>
              </div>
            </div>
          {:else if infiniteScroll && endOfResults && rows.length > 0}
            <!-- Infinite scroll: all rows loaded, explicit end marker so the user
                 knows scrolling won't fetch more (vs. "is it still loading?"). -->
            <div style="position:relative;width:100%;pointer-events:none;z-index:5">
              <div class="flex items-center justify-center py-2.5">
                <span class="rounded-full border border-border/15 bg-muted/20 px-3 py-1 text-ui-2xs text-muted-foreground">
                  End of results, {rows.length.toLocaleString()} {rows.length === 1 ? 'row' : 'rows'}
                </span>
              </div>
            </div>
          {/if}

        </div>
      {/snippet}
    </ContextMenu.Trigger>

    <!-- No [&_[data-slot=…]] density overrides here: the dense recipe now lives in
         ui/context-menu/* for every menu in the app, which is what DESIGN_SYSTEM.md
         §7 requires ("if a menu looks off, the primitive is wrong; fix it there"). -->
    <ContextMenu.Content
      onOpenAutoFocus={(e) => e.preventDefault()}
      class="min-w-52"
    >
      {#if contextIsHeader && contextHeaderCol.startsWith('__vcol__')}
        {@const vcid = contextHeaderCol.slice(8)}
        {@const vcdef = ($virtualColumnsStore[_tableKey] ?? []).find(c => c.id === vcid)}
        <ContextMenu.Item onSelect={() => runMenuAction(() => { vcolPanelOpen = true })}>
          <Pencil /> Edit column
        </ContextMenu.Item>
        {#if vcdef}
          <ContextMenu.Item onSelect={() => runMenuAction(() => virtualColumnsStore.patch(_tableKey, vcid, { enabled: !vcdef.enabled }))}>
            <EyeOff /> {vcdef.enabled ? 'Disable column' : 'Enable column'}
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Separator />
        <ContextMenu.Item
          onSelect={() => runMenuAction(() => { virtualColumnsStore.remove(_tableKey, vcid) })}
          class="text-destructive focus:text-destructive"
        >
          <Trash2 /> Delete column
        </ContextMenu.Item>
      {:else if contextIsHeader}
        {@const hcol = contextHeaderCol}
        {@const hSortInfo = _sortLookup.get(hcol)}
        {@const hSorted = !!hSortInfo}
        {@const hAsc = hSortInfo?.direction === 'asc'}
        {@const hDesc = hSortInfo?.direction === 'desc'}
        {@const hPinned = pinnedColumns.has(hcol)}
        {@const hVisIdx = visibleColumns.findIndex((c) => c.name === hcol)}
        {@const hIsFirst = hVisIdx <= 0}
        {@const hIsLast = hVisIdx < 0 || hVisIdx >= visibleColumns.length - 1}
        <ContextMenu.Item onSelect={() => runMenuAction(() => headerSortDirect(hcol, 'asc'))}>
          <ArrowUp />
          {$t('menu.sortAsc')}
          {#if hAsc}<span class="ml-auto text-ui-3xs text-primary">✓</span>{/if}
        </ContextMenu.Item>
        <ContextMenu.Item onSelect={() => runMenuAction(() => headerSortDirect(hcol, 'desc'))}>
          <ArrowDown />
          {$t('menu.sortDesc')}
          {#if hDesc}<span class="ml-auto text-ui-3xs text-primary">✓</span>{/if}
        </ContextMenu.Item>
        {#if hSorted}
          <ContextMenu.Item onSelect={() => runMenuAction(() => { if (pendingEdits.size > 0) { toast.error('Unsaved changes', { description: 'Apply or reset your edits before sorting.' }); return } onsortchange([]) })}>
            <ArrowUpDown />
            {$t('menu.clearSort')}
          </ContextMenu.Item>
        {/if}
        {#if hasTableContext}
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => runMenuAction(() => onfiltercolumn(hcol))}>
            <ListFilter />
            {$t('menu.filterByColumn')}
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Separator />
        <ContextMenu.Item onSelect={() => runMenuAction(() => toggleColumnPin(hcol))}>
          {#if hPinned}<PinOff />Unpin column{:else}<Pin />Pin column{/if}
        </ContextMenu.Item>
        <ContextMenu.Item onSelect={() => runMenuAction(() => onhidecolumn(hcol))}>
          <EyeOff />
          {$t('menu.hideColumn')}
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <!-- One "Move" entry instead of four stacked ones. Four of the menu's
             fourteen rows were the same verb with a different adverb, which is
             what a submenu is for - and it puts "Reset order" next to the actions
             that change the order rather than five rows below them. -->
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger disabled={hIsFirst && hIsLast}>
            <MoveHorizontal />
            {$t('menu.move')}
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent class="min-w-44">
            <ContextMenu.Item disabled={hIsFirst} onSelect={() => runMenuAction(() => moveColumn(hcol, 'left'))}>
              <ChevronLeft />
              {$t('menu.moveLeft')}
            </ContextMenu.Item>
            <ContextMenu.Item disabled={hIsLast} onSelect={() => runMenuAction(() => moveColumn(hcol, 'right'))}>
              <ChevronRight />
              {$t('menu.moveRight')}
            </ContextMenu.Item>
            <ContextMenu.Item disabled={hIsFirst} onSelect={() => runMenuAction(() => moveColumn(hcol, 'first'))}>
              <ChevronsLeft />
              {$t('menu.moveFirst')}
            </ContextMenu.Item>
            <ContextMenu.Item disabled={hIsLast} onSelect={() => runMenuAction(() => moveColumn(hcol, 'last'))}>
              <ChevronsRight />
              {$t('menu.moveLast')}
            </ContextMenu.Item>
            {#if columnOrder.length > 0}
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => runMenuAction(() => resetColumnOrder())}>
                <RotateCcw />
                {$t('menu.resetOrder')}
              </ContextMenu.Item>
            {/if}
          </ContextMenu.SubContent>
        </ContextMenu.Sub>
        <ContextMenu.Item onSelect={() => runMenuAction(() => resetColumnWidth(hcol))}>
          <RotateCcw />
          {$t('menu.resetWidth')}
        </ContextMenu.Item>
        <ContextMenu.Separator />
        <!-- Copying a column was only possible by selecting it first; the header
             menu is where you are already pointing at the column. -->
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger>
            <Copy />
            {$t('menu.copy')}
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent class="min-w-48">
            <ContextMenu.Item onSelect={() => runMenuAction(() => void copyColumnValues(hcol))}>
              <Copy />
              All cell values
              <span class="ml-auto text-ui-3xs tabular-nums text-muted-foreground">
                {(selected.size > 0 ? selected.size : rows.length).toLocaleString()}
              </span>
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => runMenuAction(() => void writeClipboard(hcol))}>
              <Type />
              Column name
            </ContextMenu.Item>
          </ContextMenu.SubContent>
        </ContextMenu.Sub>
        {#if hasTableContext}
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => runMenuAction(() => { statsCol = statsCol === hcol ? null : hcol })}>
            <BarChart2 />
            {$t('menu.columnStats')}
            {#if statsCol === hcol}<span class="ml-auto text-ui-3xs text-primary">✓</span>{/if}
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Separator />
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger>
            <Palette />
            {$t('menu.highlight')}
            {#if colHighlights[hcol]?.color}<span class="ml-auto size-2.5 rounded-full" style="background:{COL_HL_MAP.get(colHighlights[hcol].color)}"></span>{/if}
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent class="min-w-40">
            {#each COL_HIGHLIGHTS as h (h.id)}
              <ContextMenu.Item onSelect={() => runMenuAction(() => setColHighlight(hcol, h.id))}>
                <span class="size-3.5 shrink-0 rounded-full border border-border/40" style="background:{h.hex}"></span>
                <span data-slot="menu-label">{h.label}</span>
                {#if colHighlights[hcol]?.color === h.id}<span class="ml-auto text-ui-3xs text-primary">✓</span>{/if}
              </ContextMenu.Item>
            {/each}
            {#if colHighlights[hcol]?.color}
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => runMenuAction(() => setColHighlight(hcol, null))}>
                <Ban />
                No color
              </ContextMenu.Item>
            {/if}
          </ContextMenu.SubContent>
        </ContextMenu.Sub>
        <ContextMenu.Item onSelect={() => runMenuAction(() => openTagDialog(hcol))}>
          <Tag />
          {colHighlights[hcol]?.tag ? $t('menu.editTag') : $t('menu.tagColumn')}
        </ContextMenu.Item>
        {#if colHighlights[hcol]?.tag}
          <ContextMenu.Item onSelect={() => runMenuAction(() => setColTag(hcol, ''))}>
            <Ban />
            Remove tag
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Separator />
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger>
            <Wand2 />
            {$t('menu.transformColumn')}
            {#if colTransforms[hcol]}<span class="ml-auto text-ui-3xs text-primary">on</span>{/if}
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent class="min-w-56">
            {#if menuColTransforms.length > 0}
              {#each menuColTransforms as t (t.id)}
                <ContextMenu.Item onSelect={() => runMenuAction(() => setColTransform(hcol, t.id))}>
                  <Wand2 />
                  <span data-slot="menu-label">{t.label}</span>
                  {#if colTransforms[hcol] === t.id}<span class="ml-auto text-ui-3xs text-primary">✓</span>{/if}
                </ContextMenu.Item>
              {/each}
            {:else}
              <div class="px-2 py-1.5 text-ui-2xs italic text-muted-foreground">No transforms apply to this column</div>
            {/if}
            {#if colTransforms[hcol]}
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => runMenuAction(() => setColTransform(hcol, null))}>
                <Ban />
                {$t('menu.clearTransform')}
              </ContextMenu.Item>
            {/if}
          </ContextMenu.SubContent>
        </ContextMenu.Sub>
      {:else}
        <ContextMenu.Item onSelect={() => runMenuAction(() => openInInspector(contextRowIdx))}>
          <PanelRight />
          Open row
        </ContextMenu.Item>
        <!-- The cell, full size, in the bottom dock. It had a keyboard binding
             and no way to ask for it with the pointer, which is the hand that is
             already on a cell when you find out 28px was not enough of it. -->
        <ContextMenu.Item onSelect={() => runMenuAction(() => openCellEditor(contextRowIdx, contextColIdx))}>
          <PanelBottom />
          Preview cell
          <ContextMenu.Shortcut combo="Space" />
        </ContextMenu.Item>
        <!-- The whole row, as JSON, in the same dock. Beside Preview cell
             because it is the same question asked of the row instead of the
             cell, and it had the same problem: a binding and no way to reach it
             with the hand already holding the pointer. -->
        <ContextMenu.Item onSelect={() => runMenuAction(() => openRowJson(contextRowIdx))}>
          <Braces />
          Preview row JSON
          <ContextMenu.Shortcut combo="Alt+J" />
        </ContextMenu.Item>
        {#if menuForeignKey}
          <ContextMenu.Item
            disabled={menuCellNull}
            title={menuCellNull ? 'This value is NULL, there is no referenced row to open.' : undefined}
            onSelect={() =>
              runMenuAction(() =>
                onfollowforeignkey({
                  rowIdx: contextRowIdx,
                  colIdx: contextColIdx,
                }),
              )}
          >
            <ExternalLink />
            {menuCellNull ? 'Open referenced row, value is NULL' : 'Open referenced row'}
            {#if !menuCellNull}<ContextMenu.Shortcut combo="Mod+Enter" />{/if}
          </ContextMenu.Item>
          <!-- The same target, in a tab of its own. The pointer needs its own way
               to ask for that: ⇧↵ does it from the keyboard, and "already open"
               is exactly when you want a second copy rather than a jump. -->
          <ContextMenu.Item
            disabled={menuCellNull}
            onSelect={() =>
              runMenuAction(() =>
                onfollowforeignkey({
                  rowIdx: contextRowIdx,
                  colIdx: contextColIdx,
                  newTab: true,
                }),
              )}
          >
            <ExternalLink />
            Open in new tab
            {#if !menuCellNull}<ContextMenu.Shortcut combo="Shift+Enter" />{/if}
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Separator />
        <ContextMenu.Item onSelect={() => runMenuAction(() => toggleColumnPin(menuColName))}>
          {#if menuColPinned}<PinOff />Unpin column{:else}<Pin />Pin column{/if}
        </ContextMenu.Item>
        <ContextMenu.Separator />
        {#if hasTableContext}
          <ContextMenu.Item
            disabled={!menuEditable || readonly}
            onSelect={() => runMenuAction(() => startEdit(contextRowIdx, contextColIdx))}
          >
            <Pencil />
            Edit
            <ContextMenu.Shortcut combo="Enter" />
          </ContextMenu.Item>
        {/if}
        {#if menuCellIsArray && menuEditable && !readonly}
          <ContextMenu.Item onSelect={() => runMenuAction(() => openArrayEditor(contextRowIdx, contextColIdx))}>
            <Braces />
            Edit array…
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Item onSelect={() => runMenuAction(() => copyCellValue(contextRowIdx, contextColIdx))}>
          <Copy />
          Copy
          <ContextMenu.Shortcut combo="Mod+C" />
        </ContextMenu.Item>
        <ContextMenu.Sub>
          <ContextMenu.SubTrigger>
            <Copy />
            Copy as
          </ContextMenu.SubTrigger>
          <ContextMenu.SubContent class="min-w-44">
            <ContextMenu.Label>Cell</ContextMenu.Label>
            <ContextMenu.Item onSelect={() => runMenuAction(() => copyCellHex(contextRowIdx, contextColIdx))}>
              <Copy />
              Hex
            </ContextMenu.Item>
            <ContextMenu.Separator />
            <ContextMenu.Label>Row</ContextMenu.Label>
            <ContextMenu.Item onSelect={() => runMenuAction(() => copyAs(contextRowIdx, 'json'))}>
              <Braces />
              JSON
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => runMenuAction(() => copyAs(contextRowIdx, 'csv'))}>
              <Copy />
              CSV
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => runMenuAction(() => copyAs(contextRowIdx, 'plain'))}>
              <Copy />
              Plain text
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => runMenuAction(() => copyAs(contextRowIdx, 'markdown'))}>
              <Copy />
              Markdown table
            </ContextMenu.Item>
            {#if hasTableContext}
              <ContextMenu.Item onSelect={() => runMenuAction(() => copyAs(contextRowIdx, 'insert'))}>
                <Copy />
                INSERT statement
              </ContextMenu.Item>
            {/if}
          </ContextMenu.SubContent>
        </ContextMenu.Sub>
        {#if hasTableContext}
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger disabled={menuCellOversize}>
              <ListFilter />
              Filter
            </ContextMenu.SubTrigger>
            <!-- `quickFilter` is read ONLY inside this SubContent, which bits-ui does
                 not render until the submenu opens. That matters: building it scans
                 every loaded row (up to 2000) and JSON.stringifies each object cell,
                 uncached. Reading it in the parent content - as the old "Quick filter"
                 sub-trigger's {#if} did - paid that scan on every single right-click,
                 even when the user never went near the submenu. -->
            <ContextMenu.SubContent class="app-scroll max-h-[60vh] min-w-40 overflow-y-auto">
              <ContextMenu.Item
                onSelect={() => runMenuAction(() => onfilterbyvalue(menuColName, rows[contextRowIdx]?.[contextColIdx]))}
              >
                <ListFilter />
                {menuCellNull ? 'Is NULL' : 'By this value'}
                <ContextMenu.Shortcut combo="Alt+F" />
              </ContextMenu.Item>
              <ContextMenu.Item
                onSelect={() => runMenuAction(() => onfilterbyvalue(menuColName, rows[contextRowIdx]?.[contextColIdx], true))}
              >
                <FilterX />
                {menuCellNull ? 'Is not NULL' : 'Exclude this value'}
                <ContextMenu.Shortcut combo="Alt+E" />
              </ContextMenu.Item>
              {#if quickFilter}
                {#each quickFilter.groups as group, gi (gi)}
                  <ContextMenu.Separator />
                  {#if group.title}<ContextMenu.Label>{group.title}</ContextMenu.Label>{/if}
                  {#each group.items as item (item.key)}
                    <ContextMenu.Item onSelect={() => runMenuAction(() => onquickfilter(menuColName, item.op, item.value))}>
                      <span data-slot="menu-label">{item.label}</span>
                      {#if item.active}<Check class="ml-auto text-primary" />{/if}
                    </ContextMenu.Item>
                  {/each}
                {/each}
              {/if}
            </ContextMenu.SubContent>
          </ContextMenu.Sub>
        {/if}
        {#if menuTransforms.length > 0}
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger>
              <Wand2 />
              Transform
            </ContextMenu.SubTrigger>
            <ContextMenu.SubContent class="min-w-48">
              {#each menuTransforms as t (t.id)}
                <ContextMenu.Item onSelect={() => runMenuAction(() => runCellTransform(contextRowIdx, contextColIdx, t))}>
                  <Wand2 />
                  <span data-slot="menu-label">{t.label}</span>
                </ContextMenu.Item>
              {/each}
            </ContextMenu.SubContent>
          </ContextMenu.Sub>
        {/if}
        {#if menuGenerators.length > 0 && menuEditable && !readonly && hasTableContext}
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger>
              <Dices />
              Insert
            </ContextMenu.SubTrigger>
            <ContextMenu.SubContent class="app-scroll max-h-[60vh] min-w-52 overflow-y-auto">
              {#each menuGenerators as grp, gi (grp.group)}
                {#if gi > 0}<ContextMenu.Separator />{/if}
                {@const GIcon = grp.group === 'IDs' ? KeyRound : grp.group === 'TIME' ? Clock : Dices}
                <ContextMenu.Label>{grp.group}</ContextMenu.Label>
                {#each grp.items as g (g.id)}
                  <ContextMenu.Item onSelect={() => runMenuAction(() => insertGeneratedValue(contextRowIdx, contextColIdx, g))}>
                    <GIcon />
                    <span data-slot="menu-label">{g.label}</span>
                  </ContextMenu.Item>
                {/each}
              {/each}
            </ContextMenu.SubContent>
          </ContextMenu.Sub>
        {/if}
        <ContextMenu.Separator />
        {#if hasTableContext}
          {#if menuColNullable}
            <ContextMenu.Item
              disabled={!menuEditable || menuCellNull || readonly}
              onSelect={() => runMenuAction(() => setCellNull(contextRowIdx, contextColIdx))}
            >
              <CircleSlash />
              Set NULL
            </ContextMenu.Item>
          {/if}
          {#if selected.size > 1 && selected.has(contextRowIdx)}
            <ContextMenu.Item
              disabled={!menuEditable || menuCellOversize || readonly}
              class="whitespace-nowrap"
              onSelect={() => runMenuAction(() => fillSelectedColumn(contextColIdx, effectiveCellValue(contextRowIdx, contextColIdx)))}
            >
              <CopyPlus />
              Fill {formatCompactCount(selected.size)} rows with this value
            </ContextMenu.Item>
          {/if}
        {/if}
        {#if showRowExpand}
          <ContextMenu.Item onSelect={() => runMenuAction(() => toggleRowExpand(contextRowIdx))}>
            <Braces />
            {isRowExpanded(contextRowIdx) ? "Collapse row JSON" : "Expand"}
            <ContextMenu.Shortcut combo="Mod+E" />
          </ContextMenu.Item>
        {/if}
        <ContextMenu.Item onSelect={() => runMenuAction(() => toggleRow(contextRowIdx))}>
          <CheckSquare />
          {selected.has(contextRowIdx) ? "Deselect row" : "Select row"}
        </ContextMenu.Item>
        {#if hasTableContext}
        <!-- Insert, anchored to the row you clicked. Both open the same draft
             band under the header - "above" parks this row directly beneath it,
             "below" parks the next one there, so the new row appears where the
             label says. -->
        <ContextMenu.Item
          disabled={readonly}
          onSelect={() => runMenuAction(() => openInsertDraft(contextRowIdx))}
        >
          <ArrowUpFromLine />
          Insert row above
        </ContextMenu.Item>
        <ContextMenu.Item
          disabled={readonly}
          onSelect={() => runMenuAction(() => openInsertDraft(contextRowIdx + 1))}
        >
          <ArrowDownFromLine />
          Insert row below
        </ContextMenu.Item>
        <ContextMenu.Item
          disabled={readonly}
          onSelect={() => runMenuAction(() => duplicateRow(contextRowIdx))}
        >
          <CopyPlus />
          Duplicate row
          <ContextMenu.Shortcut combo="Alt+D" />
        </ContextMenu.Item>
        <ContextMenu.Separator />
        {#if pendingDeletes.has(contextRowIdx)}
          <ContextMenu.Item
            disabled={readonly}
            class="whitespace-nowrap"
            onSelect={() => runMenuAction(() => undoDeleteRow(contextRowIdx))}
          >
            <RotateCcw />
            Undo delete
          </ContextMenu.Item>
        {:else}
          <ContextMenu.Item
            variant="destructive"
            disabled={!hasPrimaryKey || saving || readonly}
            class="whitespace-nowrap"
            onSelect={() => runMenuAction(() => deleteRow(contextRowIdx))}
          >
            <Trash2 />
            {selected.size > 1 && selected.has(contextRowIdx)
              ? `Delete ${formatCompactCount(selected.size)} rows`
              : "Delete row"}
            <ContextMenu.Shortcut combo="Mod+Backspace" />
          </ContextMenu.Item>
        {/if}
        {/if}
      {/if}
    </ContextMenu.Content>
  </ContextMenu.Root>

<!-- Related-rows dock, sits BELOW the scroll container in this flex column, so
     it never scrolls with the grid and the grid never fights its inner scroll. -->
{#if fkSubview !== null && rows[fkSubview.rowIdx] !== undefined}
  {@const fkIdx = fkSubview.rowIdx}
  <div
    class="relative z-10 flex shrink-0 flex-col border-t border-border/60 bg-background"
    style="height:{fkDockHeight}px"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize related rows panel"
      class="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize"
      onpointerdown={(e) => startDockResize(e, 'fk')}
    ></div>
    <FkSubviewPanel
      data={fkSubview.data}
      metrics={gridMetrics}
      fkLabel={fkSubview.label}
      sourceHint={`row ${fkIdx + 1}`}
      onclose={() => { fkSubview = null }}
      onfullview={() => {
        const sv = fkSubview
        if (!sv) return
        if (sv.kind === 'reverse' && sv.relInfo) {
          // Reverse FK: navigate to fromTable with filter
          openReverseFkFullView(fkIdx, sv.relInfo)
        } else {
          // Forward FK: navigate to referenced table via normal FK nav
          onfollowforeignkey({ rowIdx: fkIdx, colIdx: sv.colIdx ?? 0 })
        }
      }}
    />
  </div>
{/if}

<!-- Full-size cell editor, the second dock. Same place, same chrome and same
     resize as the related-rows panel above - it used to be a centred modal,
     which covered the rows the value came from and took the window for one
     field. Mutually exclusive with that panel (see openCellEditor). -->
{#if cellEditorOpen}
  <div
    class="relative z-10 flex shrink-0 flex-col border-t border-border/60 bg-background"
    style="height:{cellDockHeight}px"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize cell editor panel"
      class="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize"
      onpointerdown={(e) => startDockResize(e, 'cell')}
    ></div>
    <CellEditorPanel
      bind:this={cellEditorRef}
      bind:open={cellEditorOpen}
      autofocus={cellEditorFocusOnOpen}
      colName={cellEditorName}
      colType={cellEditorType}
      value={cellEditorValue}
      sourceHint={cellEditorRow >= 0 ? `row ${cellEditorRow + 1}` : ''}
      detached={cellEditorDetached}
      readOnly={readonly || cellEditorDetached || !!cellEditorOversize || !canEditColumn(cellEditorCol)}
      oversize={cellEditorOversize}
      truncatedLoad={cellEditorTruncated}
      onloadfull={onfetchcellvalue && cellEditorRow >= 0 ? loadFullCellValue : null}

      oncommit={commitCellEditor}
      onclose={() => tick().then(() => tableContainer?.focus({ preventScroll: true }))}
    />
  </div>
{/if}

<!-- Review before any edit, insert or delete is applied. The last dock, so it
     sits on the bottom edge with the staged rows still visible above it. -->
{#if dmlPreview}
  <div
    class="relative z-10 flex shrink-0 flex-col border-t border-border/60 bg-background"
    style="height:{reviewDockHeight}px"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize review panel"
      class="absolute inset-x-0 -top-1 z-10 h-2 cursor-row-resize"
      onpointerdown={(e) => startDockResize(e, 'review')}
    ></div>
    <DmlReviewPanel
      bind:this={dmlPanel}
      bind:sql={dmlEditedSql}
      title={dmlPreview.title}
      description={dmlPreview.description}
      confirmLabel={dmlPreview.confirmLabel}
      destructive={dmlPreview.destructive}
      originalSql={dmlOriginalSql}
      edited={dmlWasEdited}
      {dialect}
      sqlHints={dmlCompletion}
      running={dmlPreviewRunning}
      onconfirm={confirmDmlPreview}
      oncancel={() => closeDmlPreview()}
    />
  </div>
{/if}
</div>

{#if statsCol && hasTableContext}
  {@const statsColInfo = columns.find(c => c.name === statsCol)}
  <ColumnStatsPanel
    {schema}
    table={tableName}
    column={statsCol}
    dataType={statsColInfo?.dataType ?? statsColInfo?.data_type ?? ""}
    onclose={() => { statsCol = null }}
  />
{/if}
{#if vcolPanelOpen}
  <VirtualColumnsPanel
    {schema}
    tableName={tableName}
    {columns}
    {rows}
    nameToIdx={_nameToActualIdx}
    onclose={() => { vcolPanelOpen = false }}
  />
{/if}

<!-- Right-click on a staged cell.
     One menu for the whole band, not one `ContextMenu.Root` per cell: the band
     is a grid, so per-cell roots would mount and tear down a menu per column on
     every keystroke. This is the same `DropdownMenu` every other menu in the app
     uses - so it dismisses, traps focus and reads the same - anchored to a
     zero-size box parked at the pointer instead of to a trigger button. -->
<DropdownMenu.Root
  open={draftMenu !== null}
  onOpenChange={(open) => {
    if (open) return
    // `onCloseAutoFocus` fires after `draftMenu` is cleared, so stash where the
    // caret has to go back to before letting go of it.
    closedFromDraft = draftMenu
    draftMenu = null
  }}
>
  <DropdownMenu.Trigger
    aria-hidden="true"
    tabindex={-1}
    class="pointer-events-none fixed size-0 opacity-0"
    style="left:{draftMenu?.x ?? 0}px; top:{draftMenu?.y ?? 0}px"
  />
  <DropdownMenu.Content
    align="start"
    sideOffset={0}
    class="min-w-48"
    onCloseAutoFocus={(e) => {
      // The anchor is a zero-size box parked at the pointer, so letting the menu
      // hand focus back to it would strand the caret on nothing. Send it to the
      // staged field the menu was opened over instead.
      e.preventDefault()
      const m = closedFromDraft
      closedFromDraft = null
      if (!m) return
      const band = document.querySelector(`[data-new-row="${m.row}"]`)
      const el = /** @type {HTMLElement|null} */ (
        band?.querySelector(`[data-new-row-input="${m.col}"]`) ?? null
      )
      el?.focus({ preventScroll: true })
    }}
  >
    {#if draftMenu}
      {@const menuActions = draftMenuActions}
      {#each menuActions as action (action.id)}
        <DropdownMenu.Item onSelect={() => action.run()}>
          <action.icon />
          {action.label}
        </DropdownMenu.Item>
      {/each}
      {#if menuActions.length}
        <DropdownMenu.Separator />
      {/if}
      <DropdownMenu.Item
        onSelect={() => {
          const v = newRowDrafts?.[draftMenu?.row ?? 0]?.[draftMenu?.col ?? ''] ?? ''
          void navigator.clipboard?.writeText(v)
        }}
      >
        <Copy />
        Copy value
      </DropdownMenu.Item>
      <DropdownMenu.Item
        onSelect={async () => {
          const m = draftMenu
          if (!m) return
          const text = await readClipboardText()
          if (text) setNewRowDraft(m.row, m.col, text)
        }}
      >
        <ClipboardPaste />
        Paste
      </DropdownMenu.Item>
      {#if draftCount > 1}
        <DropdownMenu.Item
          onSelect={() => { if (draftMenu) fillDraftColumn(draftMenu.row, draftMenu.col) }}
        >
          <Columns3 />
          <span data-slot="menu-label">Fill this column in all {draftCount} rows</span>
        </DropdownMenu.Item>
      {/if}
      <DropdownMenu.Separator />
      <DropdownMenu.Item
        onSelect={() => {
          const m = draftMenu
          if (!m) return
          const col = columns.find((c) => c.name === m.col)
          setNewRowDraft(m.row, m.col, col ? defaultInsertDraft(col, primaryKey) : '')
        }}
      >
        <Eraser />
        Clear
      </DropdownMenu.Item>
    {/if}
  </DropdownMenu.Content>
</DropdownMenu.Root>
</div>

<Dialog.Root bind:open={tagDialogOpen}>
  <Dialog.Content class="max-w-sm gap-4">
    <Dialog.Header>
      <Dialog.Title class="text-ui-sm font-semibold">Tag column</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        A short label shown on the “{tagDialogCol}” header. Leave empty to remove.
      </Dialog.Description>
    </Dialog.Header>
    <input
      bind:value={tagDialogValue}
      maxlength="24"
      spellcheck="false"
      placeholder="e.g. PII, money, deprecated"
      class= "field-surface h-9 w-full bg-background px-3 text-ui-sm text-foreground outline-none transition-[border-color,box-shadow]"
      onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); confirmTag(); } else if (e.key === 'Escape') { e.preventDefault(); tagDialogOpen = false; } }}
    />
    <Dialog.Footer class="gap-2 sm:justify-end">
      <button type= "field-surface button"class="inline-flex h-8 items-center px-3 text-ui-xs font-medium text-foreground transition-colors hover:bg-muted"onclick={() => (tagDialogOpen = false)}>Cancel</button>
      <button type="button" class="inline-flex h-8 items-center rounded-lg bg-primary px-3 text-ui-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90" onclick={confirmTag}>Save</button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<MediaLightbox
  url={lightboxUrl}
  type={lightboxType}
  onclose={() => {
    lightboxUrl = null;
  }}
/>

<VectorCellViewer
  bind:open={vectorViewerOpen}
  column={vectorViewerColName}
  dataType={vectorViewerType}
  nullable={vectorViewerNullable}
  readOnly={vectorViewerReadOnly}
  value={vectorViewerValue}
  onsave={commitVectorViewer}
/>

<GeometryCellViewer
  bind:open={geomViewerOpen}
  column={geomViewerColName}
  dataType={geomViewerType}
  nullable={geomViewerNullable}
  readOnly={geomViewerReadOnly}
  value={geomViewerValue}
  onsave={commitGeometryViewer}
/>

<ArrayCellEditor
  bind:open={arrayEditorOpen}
  column={arrayEditorColName}
  elementType={arrayEditorType}
  value={arrayEditorValue}
  onsave={commitArrayEditor}
/>
