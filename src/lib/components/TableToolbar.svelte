<script>
  import Icon from "./Icon.svelte";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Check from "@lucide/svelte/icons/check";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { Popover, PopoverTrigger, PopoverContent } from "$lib/components/ui/popover/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import SearchableMenu from "./SearchableMenu.svelte";
  import DateFilterControl from "./DateFilterControl.svelte";
  import { getColumnEnumValues } from "$lib/cell-value.js";
  import { slotRoll } from "$lib/actions/slot-text.js";
  import { cn } from "$lib/utils.js";
  import { IS_MAC, keycaps, comboTitle } from "$lib/shortcuts.js";
  import Kbd from "./Kbd.svelte";
  /** Tooltip keycaps. A control that has a shortcut should say so where the
   *  pointer already is - the shortcuts dialog is where you look when you do not
   *  know a key exists, not when you are already on the button. */
  const KEY = {
    search: comboTitle('Mod+F'),
    filter: IS_MAC ? "⌥A" : "Alt+A",
    sort: IS_MAC ? "⌥S" : "Alt+S",
    columns: IS_MAC ? "⌥C" : "Alt+C",
    reset: IS_MAC ? "⌥R" : "Alt+R",
    addRow: IS_MAC ? "⌥N" : "Alt+N",
  };
  import { GAME_WORD, CLEAR_WORD, isMagic } from '$lib/games/easter-eggs.js'
  import {
    FILTER_OPS,
    BOOL_FILTER_OPS,
    DATE_FILTER_OPS,
    NUM_FILTER_OPS,
    MAX_PAGE_SIZE,
    PAGE_SIZE_OPTIONS,
    PAGE_SIZE_ALL,
    pageSizeLabel,
    activeFilters,
    createFilter,
    ANY_COLUMN,
  } from "$lib/table-query.js";
  import { onDestroy, untrack } from "svelte";
  import { formatCompactCount } from "$lib/table-list.js";
  import { describeTableView } from "$lib/stores/table-views.js";
  import { searchOptionHotkey, SEARCH_OPTION_KEYS } from "$lib/search-options.js";

  /** @typedef {import('$lib/table-query.js').TableSort} TableSort */
  /** @typedef {import('$lib/table-query.js').TableFilter} TableFilter */
  /** @typedef {import('$lib/table-query.js').FilterOp} FilterOp */

  let {
    queryMs = 0,
    /**
     * Columns this page fetched as a preview instead of a value, with the
     * average size that earned them the treatment. Empty on every ordinary
     * table - when it is not, the chip below says so, because a column showing
     * "369 KB" where its contents should be needs to be explained once rather
     * than reported as a bug.
     * @type {{ name: string, avgBytes: number }[]}
     */
    previewColumns = [],
    page = 1,
    pageSize = 50,
    total = 0,
    loading = false,
    selectedCount = 0,
    hasPrimaryKey = false,
    deleting = false,
    columns = [],
    rowSearch = "",
    rowSort = null,
    rowFilters = [],
    onrefresh = () => {},
    /** Auto-refresh interval for THIS tab, in ms (0 = off). */
    autoRefreshMs = 0,
    onautorefreshchange = /** @type {(ms: number) => void} */ (() => {}),
    onprev = () => {},
    onnext = () => {},
    /** Keyset/cursor/temporal pagination: next/prev only, no page-jump. */
    keysetMode = false,
    /** In keyset mode, whether the current page looks full (so Next is enabled). */
    keysetHasMore = false,
    offset = 0,
    onpagechange = () => {},
    onpagesizechange = () => {},
    /** @param {number} limit @param {number} offset */
    onlimitoffsetchange = (limit, offset) => {},
    onsearchchange = () => {},
    onsortchange = () => {},
    onfilterschange = () => {},
    ondeleteselected = () => {},
    /** @type {(format: 'csv' | 'json' | 'sql' | 'tsv' | 'md' | 'jsonl') => void | Promise<void>} */
    onexport = () => {},
    onimport = () => {},
    /** Diagram exports, offered alongside the row formats while the ERD view is open. */
    /** @type {(kind: 'png' | 'copy-png' | 'svg' | 'mermaid') => void | Promise<void>} */
    onexportdiagram = () => {},
    /** Chart image exports, offered alongside the row formats in the chart view. */
    /** @type {(kind: 'png' | 'copy-png' | 'svg') => void | Promise<void>} */
    onexportchart = () => {},
    onaddrow = () => {},
    onopeninsql = () => {},
    onmagicword = /** @type {(w: 'golf' | 'crash') => void} */ (() => {}),
    /** @type {Set<string>} */
    hiddenColumns = new Set(),
    /** @type {(next: Set<string>) => void} */
    onhiddencolumnschange = () => {},
    /** Called when user picks a column from the "Jump to column" menu - the table
     *  scrolls it into view and briefly highlights it. */
    onfocuscolumn = /** @type {(name: string) => void} */ (() => {}),
    /** Virtual FK relationship columns (reverse FK badge cols) to show in the hide/show dropdown. */
    virtualRelColumns = /** @type {Array<{ label: string }>} */ ([]),
    /** User-defined virtual expression columns to show in the hide/show dropdown. */
    virtualExprCols = /** @type {Array<{id: string, name: string, enabled: boolean}>} */ ([]),
    /** Called when user toggles a virtual expression column in the dropdown. */
    ontogglevexpr = /** @type {(id: string) => void} */ (() => {}),
    filterBarOpen = $bindable(false),
    /** @type {'data' | 'structure'} */
    tableViewMode = $bindable("data"),
    /** How the data view renders the loaded page: canvas grid, JSON document,
     *  one-record-at-a-time form, or copyable text (CSV/TSV/Markdown). */
    /** @type {'table' | 'json' | 'record' | 'text' | 'chart' | 'erd'} */
    dataViewMode = $bindable("table"),
    ontogglestructure = () => {},
    /** Whether the structure view is available for the current object (false for views) */
    structureAllowed = true,
    /** Search string for column name filtering (structure mode only) */
    structureSearch = "",
    onstructuresearchchange = /** @type {(v: string) => void} */ (() => {}),
    /** When true, all write operations are disabled in the table */
    readonly = false,
    /** Infinite scroll mode - hides pagination, shows rows-loaded counter */
    infiniteScroll = false,
    oninfinitescrolltoggle = () => {},
    /** Live mode on - animates the row-count total as it changes. */
    live = false,
    /** Number of active virtual expression columns for this table (badge). */
    virtualColCount = 0,
    /** Called when user clicks the virtual columns button. */
    onopenvirtualcols = () => {},
    // ── Saved views (workflow extension) ────────────────────────────────────
    /** @type {import('$lib/stores/table-views.js').SavedTableView[]} */
    savedViews = [],
    viewsEnabled = false,
    /** @type {string | null} id of the currently applied view */
    activeViewId = null,
    onapplyview = /** @type {(view: import('$lib/stores/table-views.js').SavedTableView) => void} */ (() => {}),
    /** Clear the applied view - back to the unfiltered default. */
    onresetview = () => {},
    onsaveview = /** @type {(name: string) => void} */ (() => {}),
    ondeleteview = /** @type {(id: string) => void} */ (() => {}),
    // ── Find & replace (workflow extension) ─────────────────────────────────
    findReplaceEnabled = false,
    onfindreplace = () => {},
    // ── Search options (match case / whole word / regex) ────────────────────
    /** @type {import('$lib/search-options.js').SearchOptions} */
    searchOptions = { matchCase: false, wholeWord: false, regex: false },
    onsearchoptionschange = /** @type {(opts: import('$lib/search-options.js').SearchOptions) => void} */ (() => {}),
    /** Engine supports at least one option toggle. */
    searchOptionsSupported = false,
    /** Per-option support for the current engine (individual toggle gating). */
    searchOptionsSupport = /** @type {{ matchCase: boolean, wholeWord: boolean, regex: boolean }} */ ({
      matchCase: false,
      wholeWord: false,
      regex: false,
    }),
  } = $props();

  const ALL_SEARCH_OPTS = /** @type {const} */ ([
    { key: "matchCase", cap: "Aa", title: `Match case (${SEARCH_OPTION_KEYS.matchCase})` },
    { key: "wholeWord", cap: "ab", title: `Match whole word (${SEARCH_OPTION_KEYS.wholeWord})` },
    { key: "regex", cap: ".*", title: `Use a regular expression (${SEARCH_OPTION_KEYS.regex})` },
  ]);

  /**
   * Alt+C / Alt+W / Alt+R from inside the search field, the chords every editor's
   * find widget answers to. Options the engine cannot honor are not bound - a
   * chord that silently does nothing is worse than no chord.
   * @param {KeyboardEvent} e
   */
  /** @param {number} n */
  function fmtBytes(n) {
    if (n < 1024) return `${Math.round(n)} B`;
    if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function onSearchKeydown(e) {
    // Escape empties the box. It is what the key means in every search field
    // there is, and reaching for the ✕ with a hand already on the keyboard is
    // the kind of small friction that adds up over a day.
    if (e.key === "Escape" && localSearch) {
      e.preventDefault();
      e.stopPropagation();
      clearSearch();
      return;
    }
    const opt = searchOptionHotkey(e);
    if (!opt || !searchOptionsSupport[opt]) return;
    e.preventDefault();
    onsearchoptionschange({ ...searchOptions, [opt]: !searchOptions[opt] });
  }
  // Only the options this engine can honor (SQLite/D1 → match-case only, etc.).
  const SEARCH_OPTS = $derived(
    ALL_SEARCH_OPTS.filter((o) => searchOptionsSupport[o.key]),
  );

  let searchFocused = $state(false);
  /**
   * Whether to print the ⌘F keycaps inside the field.
   *
   * Only while it is empty and unfocused: once there is a query the clear ✕
   * takes that corner, and once the field has focus the shortcut that puts it
   * there has nothing left to say. It is a hint, not a label - `pointer-events-
   * none` so a click through it still lands in the field.
   */
  const searchHintCaps = $derived(keycaps('Mod+F'));
  const searchHint = $derived(
    tableViewMode !== 'structure' && !searchFocused && !localSearch && columns.length > 0,
  );
  /**
   * The toggles sit in the field's trailing corner, and only once there is
   * something to match.
   *
   * On an empty field they shared that corner with the ⌘F hint and the
   * placeholder, so three controls and two labels fought over 90px and the word
   * "Search…" ran under a keycap. They modify a query; with no query there is
   * nothing for them to modify.
   */
  const showSearchOpts = $derived(
    searchOptionsSupported &&
      tableViewMode !== "structure" &&
      SEARCH_OPTS.length > 0 &&
      localSearch.trim() !== "",
  );
  // Keep the field expanded while focused, typing, or adjusting options.
  const searchExpanded = $derived(
    searchFocused || localSearch.trim() !== "",
  );

  let viewsMenuOpen = $state(false);
  let viewNameDraft = $state("");

  /** Anything worth resetting: search, filters, sort, hidden columns or a non-table view mode. */
  const canResetView = $derived(
    !!(rowSearch.trim() || rowFilters.length || rowSort || hiddenColumns.size || dataViewMode !== "table"),
  );

  function commitSaveView() {
    const name = viewNameDraft.trim();
    if (!name) return;
    onsaveview(name);
    viewNameDraft = "";
    viewsMenuOpen = false;
  }

  /** @type {HTMLInputElement | null} */
  let structureSearchEl = $state(null);

  // Ctrl/Cmd+F focuses the column search input when in structure mode
  $effect(() => {
    if (tableViewMode !== "structure") return;
    /** @param {KeyboardEvent} e */
    function handler(e) {
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        structureSearchEl?.focus();
        structureSearchEl?.select();
      }
    }
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  });

  const deleteLabel = $derived(
    selectedCount === 1
      ? "Delete 1 row"
      : `Delete ${formatCompactCount(selectedCount)} rows`,
  );

  const _effectivePageSize = $derived(
    pageSize === PAGE_SIZE_ALL ? (total > 0 ? total : 1) : pageSize,
  );

  // total = -1 means the count is still being fetched in the background (the
  // row data has already loaded). Show "…" for the unknown total and keep Next
  // enabled so navigation isn't blocked during the brief counting window.
  const counting = $derived(total < 0);
  const from = $derived(total === 0 ? 0 : offset + 1);
  const to = $derived(counting ? offset + _effectivePageSize : Math.min(offset + _effectivePageSize, total));
  const pageCount = $derived(Math.max(1, Math.ceil(total / _effectivePageSize) || 1));
  const canPrev = $derived(page > 1);
  const canNext = $derived(keysetMode ? keysetHasMore : (counting || page * _effectivePageSize < total));

  const filterCount = $derived(activeFilters(rowFilters).length);
  const sortLabel = $derived(
    rowSort?.column
      ? `${rowSort.column} ${rowSort.direction === "desc" ? "↓" : "↑"}`
      : "Sort",
  );

  let sortMenuOpen = $state(false);
  let columnsMenuOpen = $state(false);
  let focusMenuOpen = $state(false);
  let limitOffsetOpen = $state(false);
  let moreMenuOpen = $state(false);
  let deleteConfirmPending = $state(false);
  /** @type {ReturnType<typeof setTimeout> | null} */
  let _deleteConfirmTimer = null;
  let draftLimit = $state(untrack(() => pageSize));
  let draftOffset = $state(0);
  let limitError = $state("");

  const hiddenCount = $derived(hiddenColumns.size);

  /** Everything the Hide all / Show all control owns: real columns and the
   *  virtual relationship columns. Virtual expr columns are left out - they are
   *  enabled/disabled through their own store, not through `hiddenColumns`. */
  const hideableColumnValues = $derived([
    ...columns.map((c) => c.name),
    ...virtualRelColumns.map((vc) => `__vrel:${vc.label}`),
  ]);

  /* The control asks "is anything still visible?", not "is anything hidden?".
     Incoming foreign keys load after the table opens, so hiding everything
     before they land leaves the relationship columns visible with something
     already hidden - and keying the label off `hiddenCount` turned the button
     into "Show all" at exactly that point, with no way left to hide them. */
  const allColumnsHidden = $derived(
    hideableColumnValues.length > 0 &&
      hideableColumnValues.every((v) => hiddenColumns.has(v)),
  );

  /** @param {string} name */
  function toggleColumn(name) {
    const next = new Set(hiddenColumns);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    onhiddencolumnschange(next);
  }

  function showAllColumns() {
    onhiddencolumnschange(new Set());
  }
  /** @type {HTMLInputElement | null} */
  let searchInputRef = $state(null);

  // Local value so the input is not controlled by the prop during typing.
  // Keeps focus when the parent triggers a re-render (e.g. loading state).
  let localSearch = $state(untrack(() => rowSearch));
  let searchDebounce = /** @type {ReturnType<typeof setTimeout> | null} */ (
    null
  );

  // A debounce surviving unmount would fire onsearchchange into whatever table
  // is active 250ms later; kill both pending timers with the component.
  onDestroy(() => {
    if (searchDebounce) clearTimeout(searchDebounce);
    if (_deleteConfirmTimer) clearTimeout(_deleteConfirmTimer);
  });

  // Sync from parent only when the prop changes from outside (e.g. table switch resets to '').
  $effect(() => {
    localSearch = rowSearch;
  });

  export function focusRowSearch() {
    searchInputRef?.focus();
    searchInputRef?.select();
  }

  /** Open the Sort menu (hotkey from the parent). */
  export function openSortMenu() {
    if (columns.length) sortMenuOpen = true;
  }
  /** Open the Columns hide/show menu (hotkey from the parent). */
  export function openColumnsMenu() {
    if (columns.length) columnsMenuOpen = true;
  }
  /**
   * Open the filter bar, seeding an empty filter row (hotkey from the parent).
   *
   * @param {string} [preferColumn] Column to seed the new row with - the one the
   *   cell cursor is on. Filtering is almost always about the column you are
   *   already looking at, and picking it again from a list of eighty is the
   *   step worth removing.
   */
  export function openFilterMenu(preferColumn = "") {
    if (!columns.length) return;
    if (!filterBarOpen) openFilterBar(preferColumn);
  }

  /** Clear the row search and focus the input (Ctrl+T shortcut). */
  export function clearRowSearch() {
    clearSearch()
    searchInputRef?.focus()
  }

  /**
   * Focus the value field of the last filter row. Scoped to that row rather than
   * to the whole bar: a row whose value is a date picker or a true/false pair has
   * no text input, and a flat query would then focus some earlier row's field.
   */
  export function focusLastFilter() {
    // Defer so the filter bar DOM has rendered
    setTimeout(() => {
      const rows = document.querySelectorAll('.studio-filter-bar [data-filter-row]');
      const row = rows[rows.length - 1];
      const el = /** @type {HTMLInputElement | HTMLButtonElement | null} */ (
        row?.querySelector('input[data-filter-value], [data-filter-value]') ?? null
      );
      el?.focus();
      if (el instanceof HTMLInputElement) el.select();
    }, 30);
  }

  /** Open the filter bar (seeding a row if empty) and land the caret in its value. */
  function openFilterBar(preferColumn = "") {
    filterBarOpen = true;
    if (rowFilters.length === 0) addFilter(preferColumn);
    focusLastFilter();
  }

  /** Page numbers shown in the page dropdown (windowed when many pages). */
  const pageMenuItems = $derived.by(() => {
    const n = pageCount;
    if (n <= 40) return Array.from({ length: n }, (_, i) => i + 1);
    const lo = Math.max(1, page - 15);
    const hi = Math.min(n, page + 15);
    return Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
  });

  // `rounded-lg`, which resolves to the same 10px as `--radius-field`: app.css asks
  // fields, dropdown triggers and buttons to share one corner, and `rounded-md` (8px)
  // left every toolbar button 2px squarer than the field it sits next to.
  const iconBtn =
    "inline-flex size-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-30";

  // ── Filter row chrome ──────────────────────────────────────────────────────
  // One border, one surface, one hover for every control in a filter row. Column,
  // condition and value each had their own pairing before, so a clause that reads
  // as one sentence was drawn as three unrelated boxes. `dark:` is repeated on the
  // surface because the Input primitive sets its own dark background.
  const fCtl =
    "field-surface h-7 bg-transparent text-ui-sm font-normal text-foreground outline-none transition-[color,background-color]";
  const fTrigger = cn(
    fCtl,
    "inline-flex items-center gap-1 px-2.5 hover:bg-muted/40 data-[state=open]:bg-muted/40 focus-visible:border-ring data-[state=open]:border-ring",
  );
  // Capped, not free-running: a value field stretched across the whole toolbar is
  // an empty box the width of the window, and it dwarfs the two controls that say
  // what it means.
  const fValue = cn(fCtl, "min-w-[9rem] max-w-[22rem] flex-1 px-2.5");

  /** Auto-refresh intervals. Nothing shorter than 5s: a refresh re-runs the page
   *  query and its count, and a faster cadence would spend more time fetching than
   *  showing. */
  const AUTO_REFRESH_OPTIONS = [
    { ms: 0, label: "Off" },
    { ms: 5_000, label: "5 seconds" },
    { ms: 10_000, label: "10 seconds" },
    { ms: 30_000, label: "30 seconds" },
    { ms: 60_000, label: "1 minute" },
    { ms: 300_000, label: "5 minutes" },
  ];
  const autoRefreshLabel = $derived(
    AUTO_REFRESH_OPTIONS.find((o) => o.ms === autoRefreshMs)?.label ?? "off",
  );

  /** @type {Array<{ id: 'table' | 'json' | 'record' | 'text' | 'chart' | 'erd' | 'map', icon: string, label: string, title?: string }>} */
  const DATA_VIEW_MODES = [
    { id: "table", icon: "table-2", label: "Table view" },
    { id: "json", icon: "braces", label: "JSON view" },
    { id: "record", icon: "layout-list", label: "Record view" },
    { id: "text", icon: "file-text", label: "Text view", title: "Text view, CSV / TSV / Markdown / JSON Lines" },
    { id: "chart", icon: "bar-chart-2", label: "Chart view", title: "Chart view, visualize the loaded rows" },
    { id: "map", icon: "globe", label: "Map view", title: "Map view: plot this table's geometry" },
    { id: "erd", icon: "network", label: "Schema diagram", title: "Schema diagram: this table and its related tables" },
  ];

  /**
   * Map view is offered only when the table has something to map. Listing it
   * everywhere would make it the one view in the menu that opens onto nothing,
   * on the overwhelming majority of tables.
   */
  const dataViewModes = $derived(
    columns.some((c) => {
      const t = String(c?.data_type ?? c?.dataType ?? "").toLowerCase();
      return t === "geometry" || t === "geography";
    })
      ? DATA_VIEW_MODES
      : DATA_VIEW_MODES.filter((m) => m.id !== "map"),
  );

  /** Export formats offered under the "Export" submenu in the more-actions menu. */
  /** @type {Array<{ id: 'png' | 'copy-png' | 'svg' | 'mermaid', label: string, icon: string }>} */
  const DIAGRAM_FORMATS = [
    { id: "copy-png", label: "Copy as PNG", icon: "copy" },
    { id: "png", label: "PNG image", icon: "file-down" },
    { id: "svg", label: "SVG vector", icon: "code-2" },
    { id: "mermaid", label: "Mermaid markdown", icon: "file-text" },
  ];

  /** Same image actions for the chart view - it has no Mermaid equivalent. */
  /** @type {Array<{ id: 'png' | 'copy-png' | 'svg', label: string, icon: string }>} */
  const CHART_FORMATS = [
    { id: "copy-png", label: "Copy as PNG", icon: "copy" },
    { id: "png", label: "PNG image", icon: "file-down" },
    { id: "svg", label: "SVG vector", icon: "code-2" },
  ];

  const EXPORT_FORMATS = [
    { id: "csv", label: "CSV" },
    { id: "json", label: "JSON" },
    { id: "sql", label: "SQL" },
    { id: "tsv", label: "TSV" },
    { id: "md", label: "Markdown" },
    { id: "jsonl", label: "JSONL" },
  ];

  // ── Searchable-menu item lists ──────────────────────────────────────────
  // Sort: two rows per column (ascending / descending), searchable by name.
  const sortItems = $derived(
    columns.flatMap((c) => [
      { value: `${c.name} ascending`, label: c.name, col: c.name, dir: "asc", keywords: [c.name], active: rowSort?.column === c.name && rowSort?.direction === "asc" },
      { value: `${c.name} descending`, label: c.name, col: c.name, dir: "desc", keywords: [c.name], active: rowSort?.column === c.name && rowSort?.direction === "desc" },
    ]),
  );

  // Columns hide/show: real columns + virtual relationship columns + virtual expr columns.
  const columnItems = $derived([
    ...columns.map((c) => ({ value: c.name, label: c.name, kind: "col", hidden: hiddenColumns.has(c.name) })),
    ...virtualRelColumns.map((vc) => ({ value: `__vrel:${vc.label}`, label: vc.label, kind: "vrel", hidden: hiddenColumns.has(`__vrel:${vc.label}`) })),
    ...virtualExprCols.map((vc) => ({ value: `__vexpr:${vc.id}`, label: `ƒ ${vc.name}`, kind: "vexpr", hidden: !vc.enabled })),
  ]);

  // Jump-to-column: only currently-visible real columns (hidden columns aren't
  // rendered on the canvas, so there's nothing to scroll to).
  const focusColumnItems = $derived(
    columns
      .filter((c) => !hiddenColumns.has(c.name))
      .map((c) => ({ value: c.name, label: c.name })),
  );

  function toggleColumnItem(/** @type {any} */ it) {
    if (it.kind === "vexpr") {
      ontogglevexpr(it.value.slice(8));
      return;
    }
    if (it.kind === "vrel") {
      const next = new Set(hiddenColumns);
      if (next.has(it.value)) next.delete(it.value); else next.add(it.value);
      onhiddencolumnschange(next);
    } else {
      toggleColumn(it.value);
    }
  }

  function hideAllColumns() {
    onhiddencolumnschange(new Set(hideableColumnValues));
  }

  /** Filter-row column options: "Any column" + every column. */
  const filterColumnItems = $derived([
    { value: ANY_COLUMN, label: "Any column", keywords: ["any", "all"] },
    ...columns.map((c) => ({ value: c.name, label: c.name })),
  ]);

  /** @param {{ id: string, column: string, op: string }} filter @param {string} v */
  function pickFilterColumn(filter, v) {
    if (!v) return;
    const newOps = opsForCol(v);
    const newOp = newOps.some((o) => o.value === filter.op) ? filter.op : defaultOpForCol(v);
    patchFilter(filter.id, { column: v, op: /** @type {FilterOp} */ (newOp), value: "" });
  }

  /** Matches the "more actions" / delete menu panel */
  const menuContent = "w-44 text-ui-sm";

  /** Compact shadcn select trigger for pagination */
  const pageSelectTrigger =
    "h-7 min-w-0 gap-1 px-2 text-ui-sm font-normal tabular-nums shadow-none";

  /** @typedef {'text' | 'boolean' | 'date' | 'number'} ColKind */

  /** @param {string} colName @returns {ColKind} */
  function getColKind(colName) {
    if (colName === ANY_COLUMN) return "text";
    const col = columns.find((c) => c.name === colName);
    const dt = (col?.dataType ?? col?.data_type ?? "")
      .toLowerCase()
      .replace(/\(.+\)$/, "")
      .trim();
    if (dt === "boolean" || dt === "bool") return "boolean";
    if (/^(date|timestamp|timestamptz|timetz|time)/.test(dt)) return "date";
    if (
      /^(int|integer|bigint|smallint|numeric|decimal|real|double|float|serial|money)/.test(
        dt,
      )
    )
      return "number";
    return "text";
  }

  const ANY_COLUMN_OPS = FILTER_OPS.filter((o) =>
    ["contains", "starts_with", "ends_with", "eq"].includes(o.value),
  );

  // Enum columns are a fixed set of values - free-text ops (contains/starts…)
  // don't make sense; offer equality + null checks and drive the value with a
  // dropdown of the actual enum members.
  const ENUM_FILTER_OPS = FILTER_OPS.filter((o) =>
    ["eq", "neq", "is_null", "is_not_null"].includes(o.value),
  );

  /** Enum members for a column, or null when it isn't an enum. @param {string} colName */
  function enumOptionsFor(colName) {
    if (colName === ANY_COLUMN) return null;
    const col = columns.find((c) => c.name === colName);
    return col ? getColumnEnumValues(col) : null;
  }

  /** @param {string} colName */
  function opsForCol(colName) {
    if (colName === ANY_COLUMN) return ANY_COLUMN_OPS;
    if (enumOptionsFor(colName)) return ENUM_FILTER_OPS;
    const kind = getColKind(colName);
    if (kind === "boolean") return BOOL_FILTER_OPS;
    if (kind === "date") return DATE_FILTER_OPS;
    if (kind === "number") return NUM_FILTER_OPS;
    return FILTER_OPS;
  }

  /** Default op when a column is first chosen */
  /** @param {string} colName @returns {import('$lib/table-query.js').FilterOp} */
  function defaultOpForCol(colName) {
    if (colName === ANY_COLUMN) return "contains";
    if (enumOptionsFor(colName)) return "eq";
    const kind = getColKind(colName);
    if (kind === "boolean") return "eq";
    if (kind === "date") return "gte";
    if (kind === "number") return "eq";
    return "contains";
  }

  /** @param {FilterOp} op */
  function filterOpLabel(op) {
    return FILTER_OPS.find((o) => o.value === op)?.label ?? op;
  }

  /** @param {string} id */
  function filterNeedsValue(id) {
    const f = rowFilters.find((x) => x.id === id);
    if (!f) return true;
    return FILTER_OPS.find((o) => o.value === f.op)?.needsValue ?? true;
  }

  /** @param {string} value */
  function handleSearchInput(value) {
    // The hidden game. Exact whole-value match so a real search that happens to
    // contain the word never fires, and handled before the debounce so the
    // filter is never actually run with it.
    // Muscle memory from a shell: type `clear`, get an empty box. Costs nothing
    // to honour and it is what your fingers meant.
    if (isMagic(value, CLEAR_WORD)) {
      clearSearch();
      return;
    }
    if (isMagic(value, GAME_WORD)) {
      localSearch = "";
      if (searchDebounce) clearTimeout(searchDebounce);
      searchDebounce = null;
      onsearchchange("");
      onmagicword("golf");
      return;
    }
    localSearch = value;
    if (searchDebounce) clearTimeout(searchDebounce);
    searchDebounce = setTimeout(() => {
      searchDebounce = null;
      onsearchchange(value);
    }, 250);
  }

  function clearSearch() {
    localSearch = "";
    if (searchDebounce) clearTimeout(searchDebounce);
    searchDebounce = null;
    onsearchchange("");
  }

  /** @param {string} [preferColumn] Seed this column instead of the first one. */
  function addFilter(preferColumn = "") {
    const wanted = preferColumn && columns.some((c) => c.name === preferColumn) ? preferColumn : "";
    const col = wanted || (columns[0]?.name ?? "");
    const op = col ? defaultOpForCol(col) : "contains";
    onfilterschange([...rowFilters, createFilter(col, op)]);
  }

  /** "+ Add filter": a new row is only useful with the caret already in it. */
  function addFilterAndFocus() {
    addFilter();
    focusLastFilter();
  }

  /** @param {string} id */
  function removeFilter(id) {
    onfilterschange(rowFilters.filter((f) => f.id !== id));
  }

  function clearFilters() {
    onfilterschange([]);
    filterBarOpen = false;
  }

  /** @param {TableFilter[]} next */
  function updateFilters(next) {
    onfilterschange(next);
  }

  /** @param {string} id @param {Partial<TableFilter>} patch */
  function patchFilter(id, patch) {
    updateFilters(
      rowFilters.map((f) => (f.id === id ? { ...f, ...patch } : f)),
    );
  }

  function clearSort() {
    onsortchange(null);
  }

  /** @param {string} column @param {'asc' | 'desc'} direction */
  function applySort(column, direction) {
    onsortchange(/** @type {TableSort} */ ({ column, direction }));
    sortMenuOpen = false;
  }

  /** Strips non-digit characters - for whole-number-only inputs (limit, offset). */
  function sanitizeDigits(val) {
    return val.replace(/\D/g, '')
  }

  /** Keeps digits, one optional leading minus, and one optional decimal point. */
  function sanitizeNumericStr(val) {
    let s = val.replace(/[^\d.-]/g, '')
    s = s.replace(/(?!^)-/g, '')
    const dot = s.indexOf('.')
    if (dot !== -1) s = s.slice(0, dot + 1) + s.slice(dot + 1).replace(/\./g, '')
    return s
  }
</script>

<!-- Controls disable for every fetch; the dim waits 200ms so a quick one never
     flashes the whole bar. CSS takes the transition from the state being
     entered, so re-enabling (back to the controls' own transitions) is instant. -->
<div class="flex shrink-0 flex-col [&_:disabled]:transition-opacity [&_:disabled]:delay-200">
  <!-- overflow-x-auto is the floor, not the plan: the breakpoints below hide
       optional controls first and the search gives way after that. It exists so
       that when a window is narrow enough (or zoomed far enough) that even the
       essentials don't fit, they stay reachable by scrolling instead of being
       clipped off the edge. Menus are portaled, so nothing is trapped by it, and
       app.css already gives this class a 4px overlay scrollbar. -->
  <header
    class="@container/tb studio-chrome studio-table-toolbar flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-b border-border bg-panel px-2"
    data-studio-chrome
  >
    <!-- Search, far left, expands on focus (wider when option toggles show) -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class={cn(
        // Shrinkable, deliberately. Everything else on this row is shrink-0, so
        // with a fixed width here the row could only overflow - which is what
        // happened at high zoom: the ten container-query breakpoints below had
        // already hidden every optional control, and the search still demanded
        // its full 13rem, so the pager clipped off the right edge. A search
        // field is the most shrinkable thing here; it gives way first, down to
        // a floor where the icon and a word of text still fit.
        "relative flex h-7 min-w-[7.5rem] shrink items-center transition-[width] duration-200",
        // Wider at both sizes: the trailing corner now holds three toggles and a
        // clear ✕ while you type, and 13rem left the query itself two words of
        // room. Still `shrink`, so a narrow pane takes it back first.
        searchExpanded ? "w-80" : "w-64",
      )}
      role="search"
      onfocusin={() => (searchFocused = true)}
      onfocusout={(e) => {
        const next = e.relatedTarget instanceof Node ? e.relatedTarget : null;
        if (!e.currentTarget.contains(next)) searchFocused = false;
      }}
    >
      <Icon name="search" class="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground" />
      {#if tableViewMode === "structure"}
        <input
          bind:this={structureSearchEl}
          type="text"
          aria-label="Search column"
          class="field-surface h-7 w-full min-w-0 bg-transparent pl-8 pr-7 font-mono text-ui-sm outline-none"
          placeholder="Search column…"
          value={structureSearch}
          oninput={(e) =>
            onstructuresearchchange(
              /** @type {HTMLInputElement} */ (e.currentTarget).value,
            )}
        />
      {:else}
        <Input
          bind:ref={searchInputRef}
          type="text"
          role="searchbox"
          aria-label="Search all columns"
          title="Search every column ({KEY.search})"
          class={cn(
            // No radius of its own: `.field-surface` supplies `--radius-field`, the
            // one corner app.css says fields, triggers and buttons all share. It was
            // `rounded-full!` - the only two pills in the component library, both of
            // them here - which made the search box visibly rounder than the filter
            // button sitting beside it. That is the exact failure the token's own
            // comment warns about, and it also made the segmented control below
            // ("they fit the field's own corner") fit nothing.
            // `border-ring!` still needs its `!` to beat the unlayered bare-input
            // frame rule in app.css; the radius no longer has anything to beat.
            "field-surface h-7 w-full min-w-0 bg-transparent pl-8 text-ui-sm outline-none",
            // Right padding is whatever the cluster in that corner occupies:
            // options trigger, keycaps, both, or neither.
            // hint (⌘F) and the toggles never coexist: the hint is for an empty
            // field, the toggles for a filled one.
            searchHint ? "pr-12" : showSearchOpts ? "pr-[7.5rem]" : "pr-7",
            localSearch.trim() && "border-ring!",
          )}
          placeholder="Search…"
          value={localSearch}
          disabled={columns.length === 0}
          oninput={(e) => handleSearchInput(e.currentTarget.value)}
          onkeydown={onSearchKeydown}
        />
      {/if}
      <!-- Right-side cluster: keycap hint + search options popover + clear (✕) -->
      <div class="absolute inset-y-0 right-1 flex items-center gap-0.5">
        {#if searchHint}
          <Kbd keys={searchHintCaps} class="pointer-events-none" aria-hidden="true" />
        {/if}
        {#if showSearchOpts}
          <!-- A segmented control, not a popover list.
               Three toggles behind a slider icon meant two clicks to reach a
               state you could not see, and a popover that covered the results
               it was about to change. `Aa`, `.*` and `ab` are what every
               editor's find widget prints, they fit the field's own corner, and
               each one's state is visible without opening anything. -->
          <!-- `rounded-sm` (6px), not `rounded-md`: this sits 4px inside the field's
               10px corner, and concentric means inner = outer - inset. -->
          <div class="flex shrink-0 items-center overflow-hidden rounded-sm border border-border/50 bg-input/40">
            {#each SEARCH_OPTS as opt, i (opt.key)}
              {@const active = searchOptions[opt.key]}
              <button
                type="button"
                aria-pressed={active}
                aria-label={opt.title}
                aria-keyshortcuts={SEARCH_OPTION_KEYS[opt.key]}
                title={opt.title}
                class={cn(
                  "inline-flex h-5 items-center justify-center px-1.5 font-mono text-ui-3xs transition-colors",
                  i > 0 && "border-l border-border/40",
                  active
                    ? "bg-primary/15 text-foreground"
                    : "text-muted-foreground hover:bg-muted/50 hover:text-foreground",
                )}
                onclick={() => onsearchoptionschange({ ...searchOptions, [opt.key]: !active })}
              >{opt.cap}</button>
            {/each}
          </div>
        {/if}
        {#if localSearch}
          <button
            type="button"
            class="inline-flex size-5 items-center justify-center rounded-sm text-muted-foreground transition-[background-color,color] duration-150 ease-out hover:bg-muted/70 hover:text-foreground"
            aria-label="Clear search (Escape, or {IS_MAC ? '⌥X' : 'Alt+X'})"
            title="Clear search (Esc)"
            onclick={clearSearch}
          >
            <Icon name="x" class="size-3" />
          </button>
        {/if}
      </div>
    </div>

    {#if tableViewMode !== "structure"}
      <!-- Action button group: views / filter / sort / columns / jump / virtual -->
      <div class="flex items-center gap-0.5">

      <!-- Saved views -->
      {#if viewsEnabled}
        <DropdownMenu.Root bind:open={viewsMenuOpen}>
          <DropdownMenu.Trigger
            class={cn(
              iconBtn,
              "shrink-0",
              savedViews.length > 0 ? "gap-1 !w-auto px-2" : "",
              (savedViews.length > 0 || viewsMenuOpen) && "bg-accent text-foreground",
              activeViewId && "text-primary",
            )}
            title={activeViewId ? "Saved views, one applied" : "Saved views"}
            disabled={loading || columns.length === 0}
          >
            <Icon name="bookmark" class="size-3.5" />
            {#if savedViews.length > 0}
              <span class="tabular-nums text-ui-2xs font-medium text-primary" aria-hidden="true">{savedViews.length}</span>
            {/if}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="start" class="min-w-64 p-0 text-ui-sm">
            <div class="flex items-center border-b border-border/50 px-3 py-1.5">
              <span class="text-ui-2xs font-medium uppercase tracking-[0.08em] text-muted-foreground">Saved views</span>
              <button
                type="button"
                class="ml-auto rounded px-1.5 py-0.5 text-ui-2xs text-muted-foreground transition-[background-color,color] hover:bg-accent hover:text-foreground"
                title="Back to the unfiltered default"
                onclick={() => { onresetview(); viewsMenuOpen = false; }}
              >
                Reset
              </button>
            </div>
            {#if savedViews.length === 0}
              <p class="px-3 py-3 text-center text-ui-xs leading-relaxed text-muted-foreground">
                Set up filters, sort or hidden columns, then save the combination as a view.
              </p>
            {:else}
              <div class="app-scroll max-h-64 overflow-y-auto p-1">
                {#each savedViews as v (v.id)}
                  {@const active = v.id === activeViewId}
                  <div
                    class={cn(
                      'group/view flex items-center rounded-md transition-colors',
                      active ? 'bg-accent/70' : 'hover:bg-accent/50',
                    )}
                  >
                    <button
                      type="button"
                      class="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left"
                      title={active ? 'Applied, click to reset to default' : 'Apply view'}
                      onclick={() => { active ? onresetview() : onapplyview(v); viewsMenuOpen = false; }}
                    >
                      <Icon name="bookmark" class={cn('size-3.5 shrink-0', active ? 'text-primary' : 'text-muted-foreground')} />
                      <span class="flex min-w-0 flex-1 flex-col">
                        <span class={cn('truncate text-ui-xs', active ? 'font-medium text-foreground' : 'text-foreground/85')}>{v.name}</span>
                        {#if describeTableView(v)}
                          <span class="truncate text-ui-3xs leading-tight text-muted-foreground">{describeTableView(v)}</span>
                        {/if}
                      </span>
                      {#if active}
                        <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                      {/if}
                    </button>
                    <button
                      type="button"
                      class="mr-1 inline-flex size-5 shrink-0 items-center justify-center rounded text-transparent transition-colors hover:!text-destructive group-hover/view:text-muted-foreground"
                      title="Delete view"
                      onclick={() => ondeleteview(v.id)}
                    >
                      <Icon name="trash-2" class="size-3" />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
            <div class="flex items-center gap-1.5 border-t border-border/50 p-1.5">
              <input
                type="text"
                placeholder="Save current as…"
                bind:value={viewNameDraft}
                class= "field-surface h-7 w-full min-w-0 flex-1 bg-transparent px-2 text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
                onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); commitSaveView(); } }}
              />
              <button
                type="button"
                class="inline-flex h-7 shrink-0 items-center rounded-md bg-primary px-2.5 text-ui-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:pointer-events-none disabled:opacity-40"
                disabled={!viewNameDraft.trim()}
                onclick={commitSaveView}
              >
                Save
              </button>
            </div>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      {/if}

      <!-- Filter -->
      <button
        type="button"
        class={cn(
          iconBtn,
          "shrink-0",
          filterCount > 0 ? "gap-1 !w-auto px-2" : "",
          (filterCount > 0 || filterBarOpen) && "bg-accent text-foreground",
        )}
        title="Filter rows ({KEY.filter})"
        disabled={loading || columns.length === 0}
        onclick={() => {
          if (filterBarOpen) filterBarOpen = false;
          else openFilterBar();
        }}
      >
        <Icon name="list-filter" class="size-3.5" />
        {#if filterCount > 0}
          <span class="tabular-nums text-ui-2xs font-medium text-primary" aria-hidden="true">{formatCompactCount(filterCount)}</span>
        {/if}
      </button>

      <!-- Sort -->
      <SearchableMenu
        bind:open={sortMenuOpen}
        items={sortItems}
        placeholder="Sort by column…"
        contentClass="w-60"
        onselect={(it) => applySort(it.col, it.dir)}
      >
        {#snippet trigger(props)}
          <button
            {...props}
            class={cn(iconBtn, "shrink-0 @max-[420px]/tb:hidden", (rowSort?.column || sortMenuOpen) && "bg-accent text-foreground")}
            title="{sortLabel} ({KEY.sort})"
            disabled={loading || columns.length === 0}
          >
            <Icon name="arrow-up-down" class="size-3.5" />
          </button>
        {/snippet}
        {#snippet header()}
          {#if rowSort?.column}
            <button
              type="button"
              class="flex w-full items-center gap-1.5 border-b border-border/40 px-3 py-1.5 text-left text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
              onclick={() => { clearSort(); sortMenuOpen = false; }}
            >
              <Icon name="x" class="size-3.5" /> Clear sort
            </button>
          {/if}
        {/snippet}
        {#snippet item(it)}
          {#if it.dir === "asc"}<Icon name="arrow-up" class="size-3.5 text-muted-foreground" />{:else}<Icon name="arrow-down" class="size-3.5 text-muted-foreground" />{/if}
          <span class="min-w-0 flex-1 truncate">{it.label}</span>
          <span class="shrink-0 text-ui-3xs text-muted-foreground">{it.dir === "asc" ? "Asc" : "Desc"}</span>
          {#if it.active}<span class="shrink-0 text-primary">✓</span>{/if}
        {/snippet}
      </SearchableMenu>

      <!-- Columns -->
      <SearchableMenu
        bind:open={columnsMenuOpen}
        items={columnItems}
        placeholder="Search columns…"
        contentClass="w-56"
        closeOnSelect={false}
        onselect={toggleColumnItem}
      >
        {#snippet trigger(props)}
          <button
            {...props}
            class={cn(iconBtn, "shrink-0 @max-[460px]/tb:hidden", hiddenCount > 0 ? "gap-1 w-auto px-2" : "", (hiddenCount > 0 || columnsMenuOpen) && "bg-accent text-foreground")}
            title="Show / hide columns ({KEY.columns})"
            aria-label="Show or hide columns"
            disabled={loading || columns.length === 0}
          >
            <Icon name={hiddenCount > 0 ? "eye-off" : "eye"} class="size-3.5" />
            {#if hiddenCount > 0}
              <span class="tabular-nums text-ui-2xs font-medium text-primary" aria-hidden="true">{hiddenCount}</span>
            {/if}
          </button>
        {/snippet}
        {#snippet header()}
          <div class="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
            <span class="text-ui-2xs font-medium uppercase tracking-wide text-muted-foreground">Columns</span>
            <!-- Both actions, whenever both mean something. One toggle that
                 flipped on "anything hidden" made "Hide all" unreachable as
                 soon as a single column was hidden. -->
            <div class="flex shrink-0 items-center gap-2">
              {#if !allColumnsHidden}
                <button
                  type="button"
                  class="text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
                  onclick={hideAllColumns}
                >
                  Hide all
                </button>
              {/if}
              {#if hiddenCount > 0}
                <button
                  type="button"
                  class="text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
                  onclick={showAllColumns}
                >
                  Show all
                </button>
              {/if}
            </div>
          </div>
        {/snippet}
        {#snippet item(it)}
          {#if it.hidden}
            <Icon name="eye-off" class="size-3.5 text-muted-foreground" />
          {:else if it.kind === "vrel"}
            <Icon name="link-2" class="size-3.5 text-primary" />
          {:else if it.kind === "vexpr"}
            <Icon name="eye" class="size-3.5 text-primary" />
          {:else}
            <Icon name="eye" class="size-3.5" />
          {/if}
          <span class={cn("min-w-0 flex-1 truncate", it.hidden && "text-muted-foreground")}>{it.label}</span>
          {#if it.kind === "vrel"}<span class="shrink-0 text-ui-3xs text-muted-foreground">rel</span>{/if}
          {#if it.kind === "vexpr"}<span class="shrink-0 text-ui-3xs text-primary">expr</span>{/if}
        {/snippet}
      </SearchableMenu>

      <!-- Jump to column -->
      <SearchableMenu
        bind:open={focusMenuOpen}
        items={focusColumnItems}
        placeholder="Jump to column…"
        contentClass="w-56"
        onselect={(it) => onfocuscolumn(it.value)}
      >
        {#snippet trigger(props)}
          <button
            {...props}
            class={cn(iconBtn, "shrink-0 @max-[500px]/tb:hidden", focusMenuOpen && "bg-accent text-foreground")}
            title="Jump to column"
            disabled={loading || columns.length === 0}
          >
            <Icon name="crosshair" class="size-3.5" />
          </button>
        {/snippet}
        {#snippet header()}
          <div class="border-b border-border/40 px-3 py-1.5">
            <span class="text-ui-2xs font-medium uppercase tracking-wide text-muted-foreground">Jump to column</span>
          </div>
        {/snippet}
        {#snippet item(it)}
          <Icon name="crosshair" class="size-3.5 text-muted-foreground" />
          <span class="min-w-0 flex-1 truncate">{it.label}</span>
        {/snippet}
      </SearchableMenu>

      <!-- Open in SQL editor: the view (search, filters, sort, columns) as a
           SELECT. Here and in the ⋯ menu. Virtual columns stay in the menu
           only, a once-a-session action. -->
      <button
        type="button"
        class={cn(iconBtn, "shrink-0 @max-[540px]/tb:hidden")}
        title="Open in SQL editor"
        aria-label="Open in SQL editor"
        disabled={loading || columns.length === 0}
        onclick={onopeninsql}
      >
        <Icon name="terminal" class="size-3.5" />
      </button>

      <!-- Reset everything, only appears when something is non-default -->
      {#if canResetView}
        <button
          type="button"
          class={cn(iconBtn, "shrink-0")}
          title="Reset view: clear search, filters, sort, hidden columns and view mode ({KEY.reset})"
          disabled={loading}
          onclick={onresetview}
        >
          <Icon name="rotate-ccw" class="size-3.5" />
        </button>
      {/if}

      </div><!-- /action group -->

      <span class="mx-0.5 h-4 w-px shrink-0 bg-border/60"></span>

      <!-- Add row -->
      <button
        type="button"
        class= "field-surface inline-flex h-7 shrink-0 items-center gap-1 px-2 text-ui-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-30"
        disabled={loading || columns.length === 0 || readonly}
        title={readonly ? "Read-only mode" : `Stage a new row (${KEY.addRow}), again for another`}
        onclick={onaddrow}
      >
        <Icon name="plus" class="size-3.5 shrink-0" />
        <span class="@max-[440px]/tb:hidden">Add</span>
      </button>
    {/if}

    <!-- Spacer -->
    <div class="flex-1"></div>

    {#if previewColumns.length && tableViewMode !== "structure"}
      <!-- What the page did NOT fetch, and why. One chip, no dialog: it is a
           fact about this table, not a decision anyone has to make. -->
      <span
        class="flex shrink-0 items-center gap-1 rounded-[4px] border border-border/50 bg-muted/30 px-1.5 py-px font-mono text-ui-3xs text-muted-foreground @max-[760px]/tb:hidden"
        title={`Fetched as previews, not values: ${previewColumns
          .map((c) => `${c.name} (~${fmtBytes(c.avgBytes)}/row)`)
          .join(', ')}.\n\nA page of these would move ${fmtBytes(
          previewColumns.reduce((n, c) => n + c.avgBytes, 0) * Math.max(1, to - from + 1),
        )}. Open a cell (Space) and press Load to read one in full.`}
      >
        <Icon name="eye-off" class="size-3 shrink-0" />
        {previewColumns.length === 1 ? previewColumns[0].name : `${previewColumns.length} columns`} previewed
      </span>
    {/if}

    {#if tableViewMode !== "structure"}
      {#if infiniteScroll}
        {#if total > 0 || counting}
          <span
            class="flex shrink-0 items-center gap-1 font-mono text-ui-xs tabular-nums @max-[600px]/tb:hidden"
            title="{to.toLocaleString('en-US')} of {counting ? 'counting…' : total.toLocaleString('en-US') + ' rows'} loaded{queryMs > 0 ? ` · ${queryMs}ms` : ''}"
          >
            <span class="text-foreground/65">{to.toLocaleString("en-US")}</span>
            <span class="text-muted-foreground">of {counting ? "…" : total.toLocaleString("en-US")} loaded</span>
          </span>
        {/if}
      {:else}
        {#if total > 0 || counting}
          <span
            class="flex shrink-0 items-center gap-1 font-mono text-ui-xs tabular-nums @max-[600px]/tb:hidden"
            title="{from.toLocaleString('en-US')}-{to.toLocaleString('en-US')} of {counting ? 'counting…' : total.toLocaleString('en-US') + ' rows'}{queryMs > 0 ? ` · ${queryMs}ms` : ''}"
          >
            <span class="text-foreground/65">{from.toLocaleString("en-US")}-{to.toLocaleString("en-US")}</span>
            {#if live && !counting}
              <span class="text-muted-foreground">of <span class="inline-block tabular-nums" use:slotRoll={total.toLocaleString("en-US")}></span></span>
            {:else}
              <span class="text-muted-foreground">of {counting ? "…" : total.toLocaleString("en-US")}</span>
            {/if}
          </span>
        {/if}

        <!-- divider before the pagination cluster (only when the range readout shows) -->
        <span class="mx-0.5 h-4 w-px shrink-0 bg-border/50 @max-[600px]/tb:hidden"></span>

        <div class="flex shrink-0 items-center gap-1.5">
        <Select.Root
          type="single"
          value={String(pageSize)}
          onValueChange={(v) => { if (v) onpagesizechange(Number(v)); }}
          disabled={loading}
        >
          <Select.Trigger size="sm" class={pageSelectTrigger} title="Rows per page" aria-label="Rows per page">
            {pageSizeLabel(pageSize)}
          </Select.Trigger>
          <Select.Content align="end" class="min-w-0">
            {#each PAGE_SIZE_OPTIONS as size (size)}
              <Select.Item value={String(size)} label={pageSizeLabel(size)} />
            {/each}
          </Select.Content>
        </Select.Root>

        <!-- Page picker + arrows only exist when there is something to paginate
             (more than one page, or the count is still unknown). A single-page
             table shows just the range readout and the page-size select. -->
        {#if keysetMode}
          <!-- Cursor pagination: no random page-jump (that's offset's job), just
               a position readout + prev/next. -->
          <span class="shrink-0 px-1 font-mono text-ui-xs tabular-nums text-muted-foreground" title="Cursor pagination, page {page}">
            Page {page}
          </span>

          <button
            type="button"
            class={iconBtn}
            disabled={!canPrev || loading}
            onclick={onprev}
            aria-label="Previous page"
          >
            <Icon name="chevron-left" class="size-3.5" />
          </button>
          <button
            type="button"
            class={iconBtn}
            disabled={!canNext || loading}
            onclick={onnext}
            aria-label="Next page"
          >
            <Icon name="chevron-right" class="size-3.5" />
          </button>
        {:else if counting || pageCount > 1}
          <Select.Root
            type="single"
            value={String(page)}
            onValueChange={(v) => { if (v) onpagechange(Number(v)); }}
            disabled={loading || total === 0}
          >
            <Select.Trigger size="sm" class={pageSelectTrigger} title="Go to page" aria-label="Go to page">
              {page}
            </Select.Trigger>
            <Select.Content align="end" class="max-h-56">
              {#each pageMenuItems as p (p)}
                <Select.Item value={String(p)} label={String(p)} />
              {/each}
            </Select.Content>
          </Select.Root>

          <span
            class="shrink-0 text-ui-xs text-muted-foreground tabular-nums @max-[500px]/tb:hidden"
            title={counting ? "counting…" : pageCount.toLocaleString("en-US")}
          >of {counting ? "…" : formatCompactCount(pageCount)}</span>

          <button
            type="button"
            class={iconBtn}
            disabled={!canPrev || loading}
            onclick={onprev}
            aria-label="Previous page"
          >
            <Icon name="chevron-left" class="size-3.5" />
          </button>
          <button
            type="button"
            class={iconBtn}
            disabled={!canNext || loading}
            onclick={onnext}
            aria-label="Next page"
          >
            <Icon name="chevron-right" class="size-3.5" />
          </button>
        {/if}
        </div>
      {/if}

    {/if}

    <!-- Refresh + auto-refresh interval read as ONE control: the caret configures
         the button it's attached to, so they share a single rounded outline and the
         halves only differ on hover. A separate free-floating caret button looked
         like a second, unrelated control competing with the refresh icon.
         Armed state is carried by the caret's colour rather than a badge - there
         is no room for a dot at this size, and the tooltip states the interval. -->
    <div class="flex shrink-0 items-stretch">
      <button
        type="button"
        class={cn(iconBtn, "w-6 rounded-r-none")}
        disabled={loading}
        onclick={onrefresh}
        title={autoRefreshMs > 0 ? `Refresh data (⌘R) · auto every ${autoRefreshLabel}` : "Refresh data (⌘R)"}
        aria-label="Refresh data"
      >
        <Icon name="refresh-cw" class={cn("size-3.5", loading && "animate-spin")} />
      </button>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <button
              {...props}
              type="button"
              class={cn(
                iconBtn,
                "h-7 w-4 rounded-l-none",
                autoRefreshMs > 0 && "text-primary hover:text-primary",
              )}
              title={autoRefreshMs > 0 ? `Auto-refresh: every ${autoRefreshLabel}` : "Auto-refresh: off"}
              aria-label="Auto-refresh interval"
            >
              <Icon name="chevron-down" class="size-3" />
            </button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-44">
          <!-- GroupHeading needs a Group parent: bits-ui reads the group context from
               it, and rendering one bare throws "Menu.Group not found". -->
          <DropdownMenu.Group>
            <DropdownMenu.GroupHeading class="text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
              Auto-refresh
            </DropdownMenu.GroupHeading>
            {#each AUTO_REFRESH_OPTIONS as opt (opt.ms)}
              <DropdownMenu.Item onSelect={() => onautorefreshchange(opt.ms)}>
                <span class="flex-1">{opt.label}</span>
                {#if autoRefreshMs === opt.ms}
                  <Icon name="check" class="size-3.5 text-primary" />
                {/if}
              </DropdownMenu.Item>
            {/each}
          </DropdownMenu.Group>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>

    <!-- Custom limit / offset -->
    <DropdownMenu.Root bind:open={limitOffsetOpen}
      onOpenChange={(open) => {
        if (open) {
          draftLimit = pageSize === PAGE_SIZE_ALL ? _effectivePageSize : pageSize;
          draftOffset = offset;
        }
      }}
    >
      <DropdownMenu.Trigger
        class={cn(iconBtn, "shrink-0", limitOffsetOpen && "bg-accent text-foreground")}
        title="Custom limit & offset"
        aria-label="Custom limit & offset"
        disabled={loading || tableViewMode === "structure" || total === 0}
      >
        <Icon name="sliders-horizontal" class="size-3.5" />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="min-w-52 p-0 text-ui-sm">
        <div class="border-b border-border px-3 py-2.5">
          <p class="font-medium text-foreground">Pagination</p>
        </div>
        <div class="flex flex-col gap-3 p-3">
          <label class="flex flex-col gap-1">
            <span class="text-ui-xs text-muted-foreground">Limit</span>
            <Input
              class={cn(
                "h-7 font-mono text-ui-sm",
                limitError && "border-destructive focus-visible:ring-destructive/30",
              )}
              type="text"
              inputmode="numeric"
              value={draftLimit}
              placeholder="e.g. 50"
              oninput={(e) => {
                const raw = sanitizeDigits(e.currentTarget.value)
                e.currentTarget.value = raw
                const v = Math.max(1, Number(raw) || 1);
                draftLimit = v;
                limitError = v > MAX_PAGE_SIZE
                  ? `Maximum is ${MAX_PAGE_SIZE.toLocaleString()} rows`
                  : "";
              }}
            />
            {#if limitError}
              <p class="text-ui-xs text-destructive">{limitError}</p>
            {/if}
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-ui-xs text-muted-foreground">Offset (skip rows)</span>
            <Input
              class="h-7 font-mono text-ui-sm"
              type="text"
              inputmode="numeric"
              value={draftOffset}
              placeholder="e.g. 0"
              oninput={(e) => {
                const raw = sanitizeDigits(e.currentTarget.value)
                e.currentTarget.value = raw
                draftOffset = Math.max(0, Number(raw) || 0);
              }}
            />
          </label>
          <div class="flex gap-1.5">
            <button
              type="button"
              class= "field-surface inline-flex flex-1 h-7 items-center justify-center text-ui-sm text-muted-foreground hover:bg-accent hover:text-foreground"
              onclick={() => { limitOffsetOpen = false; limitError = ""; }}
            >Cancel</button>
            <button
              type="button"
              class="inline-flex flex-1 h-7 items-center justify-center rounded-md bg-primary text-ui-sm font-medium text-primary-foreground hover:opacity-90 disabled:opacity-40 disabled:pointer-events-none"
              disabled={!!limitError || draftLimit < 1}
              onclick={() => {
                const l = Math.max(1, Math.floor(draftLimit));
                const o = Math.max(0, Math.floor(draftOffset));
                if (l > MAX_PAGE_SIZE) {
                  limitError = `Maximum is ${MAX_PAGE_SIZE.toLocaleString()} rows`;
                  return;
                }
                onlimitoffsetchange(l, o);
                limitOffsetOpen = false;
                limitError = "";
              }}
            >Apply</button>
          </div>
        </div>
      </DropdownMenu.Content>
    </DropdownMenu.Root>

    <!-- ⋯ More menu: structure / ∞ / export / delete -->
    <DropdownMenu.Root
      bind:open={moreMenuOpen}
      onOpenChange={(open) => {
        if (!open) {
          deleteConfirmPending = false;
          if (_deleteConfirmTimer) { clearTimeout(_deleteConfirmTimer); _deleteConfirmTimer = null; }
        }
      }}
    >
      <DropdownMenu.Trigger
        class={cn(iconBtn, "shrink-0")}
        title="More actions"
        disabled={loading || deleting}
      >
        <Icon name="more-horizontal" class="size-3.5" />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="min-w-56 [&_[data-slot=dropdown-menu-item]]:whitespace-nowrap [&_[data-slot=dropdown-menu-radio-item]]:whitespace-nowrap">
        {#if structureAllowed}
          <DropdownMenu.Item onSelect={ontogglestructure}>
            <Icon name="layout-list" class="size-3.5" />
            {tableViewMode === "structure" ? "View Data" : "View Structure"}
          </DropdownMenu.Item>
          {#if tableViewMode !== "structure"}
            <DropdownMenu.Separator />
          {/if}
        {/if}
        {#if tableViewMode !== "structure"}
          <DropdownMenu.RadioGroup
            value={dataViewMode}
            onValueChange={(v) => (dataViewMode = /** @type {'table' | 'json' | 'record' | 'text' | 'chart' | 'erd'} */ (v))}
          >
            {#each dataViewModes as m (m.id)}
              <DropdownMenu.RadioItem value={m.id} disabled={columns.length === 0}>
                <Icon name={m.icon} class="size-3.5" />
                {m.label}
              </DropdownMenu.RadioItem>
            {/each}
          </DropdownMenu.RadioGroup>
          <DropdownMenu.Separator />
          {#if findReplaceEnabled}
            <DropdownMenu.Item disabled={total === 0 || readonly || !hasPrimaryKey} onSelect={onfindreplace}>
              <Icon name="replace" class="size-3.5" />
              Find & replace…
              <DropdownMenu.Shortcut combo={IS_MAC ? "Mod+Alt+F" : "Mod+H"} />
            </DropdownMenu.Item>
          {/if}
          <DropdownMenu.Item disabled={loading || columns.length === 0} onSelect={onopenvirtualcols}>
            <Icon name="function-square" class="size-3.5" />
            Virtual columns…
            {#if virtualColCount > 0}
              <span class="ml-auto font-mono text-ui-2xs tabular-nums text-primary">{virtualColCount}</span>
            {/if}
          </DropdownMenu.Item>
          <DropdownMenu.Item disabled={loading || columns.length === 0} onSelect={onopeninsql}>
            <Icon name="terminal" class="size-3.5" />
            Open in SQL editor
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Item onSelect={oninfinitescrolltoggle}>
            <Icon name="infinity" class="size-3.5" />
            Infinite scroll
            {#if infiniteScroll}
              <span class="ml-auto text-ui-3xs text-primary">✓</span>
            {/if}
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          {#if selectedCount > 0}
            <DropdownMenu.Label class="text-ui-xs font-normal text-muted-foreground">
              {selectedCount} row{selectedCount === 1 ? "" : "s"} selected
            </DropdownMenu.Label>
          {/if}
          <DropdownMenu.Sub>
            <DropdownMenu.SubTrigger disabled={total === 0 && dataViewMode !== 'erd' && dataViewMode !== 'chart'}>
              <Icon name="file-down" class="size-3.5" />
              Export
            </DropdownMenu.SubTrigger>
            <DropdownMenu.SubContent class="min-w-44">
              <!-- GroupHeading needs a Group parent: bits-ui reads the group context
                   from it, and rendering one bare throws "Menu.Group not found". -->
              {#if dataViewMode === 'erd'}
                <DropdownMenu.Group>
                  <DropdownMenu.GroupHeading class="text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
                    Diagram
                  </DropdownMenu.GroupHeading>
                  {#each DIAGRAM_FORMATS as fmt (fmt.id)}
                    <DropdownMenu.Item onSelect={() => onexportdiagram(fmt.id)}>
                      <Icon name={fmt.icon} class="size-3.5" />
                      {fmt.label}
                    </DropdownMenu.Item>
                  {/each}
                </DropdownMenu.Group>
                <DropdownMenu.Separator />
              {/if}
              {#if dataViewMode === 'chart'}
                <DropdownMenu.Group>
                  <DropdownMenu.GroupHeading class="text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
                    Chart
                  </DropdownMenu.GroupHeading>
                  {#each CHART_FORMATS as fmt (fmt.id)}
                    <DropdownMenu.Item onSelect={() => onexportchart(fmt.id)}>
                      <Icon name={fmt.icon} class="size-3.5" />
                      {fmt.label}
                    </DropdownMenu.Item>
                  {/each}
                </DropdownMenu.Group>
                <DropdownMenu.Separator />
              {/if}
              <DropdownMenu.Group>
                {#if dataViewMode === 'erd' || dataViewMode === 'chart'}
                  <DropdownMenu.GroupHeading class="text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
                    Rows
                  </DropdownMenu.GroupHeading>
                {/if}
                {#each EXPORT_FORMATS as fmt (fmt.id)}
                  <DropdownMenu.Item disabled={total === 0} onSelect={() => onexport(fmt.id)}>
                    <Icon name="file-down" class="size-3.5" />
                    {fmt.label}
                  </DropdownMenu.Item>
                {/each}
              </DropdownMenu.Group>
            </DropdownMenu.SubContent>
          </DropdownMenu.Sub>
          <DropdownMenu.Item disabled={readonly} onSelect={() => onimport()}>
            <Icon name="file-up" class="size-3.5" />
            Import data…
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Item
            variant="destructive"
            disabled={selectedCount === 0 || !hasPrimaryKey || deleting || readonly}
            class={deleteConfirmPending ? "animate-pulse" : ""}
            onSelect={(e) => {
              if (!deleteConfirmPending) {
                e.preventDefault();
                deleteConfirmPending = true;
                if (_deleteConfirmTimer) clearTimeout(_deleteConfirmTimer);
                _deleteConfirmTimer = setTimeout(() => {
                  deleteConfirmPending = false;
                  _deleteConfirmTimer = null;
                }, 2500);
              } else {
                deleteConfirmPending = false;
                if (_deleteConfirmTimer) { clearTimeout(_deleteConfirmTimer); _deleteConfirmTimer = null; }
                ondeleteselected();
              }
            }}
          >
            <Icon name="trash-2" />
            {deleteConfirmPending ? "Click again to confirm" : deleteLabel}
            <DropdownMenu.Shortcut combo="Mod+Backspace" />
          </DropdownMenu.Item>
        {/if}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </header>

  <!-- Inline filter bar -->
  {#if filterBarOpen && columns.length > 0 && tableViewMode !== "structure"}
    <div class="studio-filter-bar border-b border-border/50 bg-panel">
      {#each rowFilters as filter, i (filter.id)}
        {@const colKind = getColKind(filter.column)}
        {@const colOps = opsForCol(filter.column)}
        {@const enumOpts = enumOptionsFor(filter.column)}
        <div
          data-filter-row
          class="flex items-center gap-1.5 border-b border-border/30 px-3 py-1.5 last:border-b-0"
        >
          <button
            type="button"
            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,transform] hover:bg-destructive/10 hover:text-destructive active:scale-[0.94]"
            aria-label="Remove filter"
            onclick={() => removeFilter(filter.id)}
          >
            <Icon name="x" class="size-3" />
          </button>
          {#if i === 0}
            <span
              class="inline-flex h-7 w-12 shrink-0 select-none items-center justify-center font-mono text-ui-2xs uppercase tracking-wide text-muted-foreground"
              >where</span
            >
          {:else}
            <button
              type="button"
              class= "field-surface inline-flex h-7 w-12 shrink-0 items-center justify-center bg-background dark:bg-background font-mono text-ui-2xs font-semibold uppercase tracking-wide text-muted-foreground transition-[color,background-color,border-color] hover: hover:bg-muted/40 hover:text-foreground"
              title="Toggle AND / OR"
              onclick={() =>
                patchFilter(filter.id, {
                  conjunct: filter.conjunct === "or" ? "and" : "or",
                })}
            >
              {filter.conjunct === "or" ? "or" : "and"}
            </button>
          {/if}
          <SearchableMenu
            items={filterColumnItems}
            placeholder="Search columns…"
            contentClass="w-52"
            onselect={(it) => pickFilterColumn(filter, it.value)}
          >
            {#snippet trigger(props)}
              <button
                {...props}
                type="button"
                class={cn(fTrigger, "w-36 shrink-0")}
                aria-label="Filter column"
              >
                <span class="min-w-0 flex-1 truncate text-left">
                  {filter.column === ANY_COLUMN ? "Any column" : filter.column || "Column"}
                </span>
                <Icon name="chevron-down" class="size-3 shrink-0 opacity-50" />
              </button>
            {/snippet}
            {#snippet item(it)}
              <span class="min-w-0 flex-1 truncate">{it.label}</span>
              {#if filter.column === it.value}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
            {/snippet}
          </SearchableMenu>
          <SearchableMenu
            items={colOps}
            placeholder="Search conditions…"
            contentClass="w-52"
            onselect={(it) => patchFilter(filter.id, { op: /** @type {FilterOp} */ (it.value), value: "" })}
          >
            {#snippet trigger(props)}
              <button
                {...props}
                type="button"
                class={cn(fTrigger, "w-32 shrink-0")}
                aria-label="Filter condition"
              >
                <span class="min-w-0 flex-1 truncate text-left">{filterOpLabel(filter.op)}</span>
                <Icon name="chevron-down" class="size-3 shrink-0 opacity-50" />
              </button>
            {/snippet}
            {#snippet item(it)}
              <span class="min-w-0 flex-1 truncate">{it.label}</span>
              {#if filter.op === it.value}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
            {/snippet}
          </SearchableMenu>
          {#if filterNeedsValue(filter.id)}
            {#if colKind === "boolean"}
              <div class="flex gap-1">
                {#each [{ label: "True", value: "true" }, { label: "False", value: "false" }] as opt (opt.value)}
                  <button
                    type="button"
                    class={cn(
                      "inline-flex h-7 items-center gap-1 rounded-md border px-2.5 text-ui-sm transition-[color,background-color,border-color]",
                      filter.value === opt.value
                        ? "border-primary bg-primary text-primary-foreground"
                        : "border-border/60 bg-background text-muted-foreground hover:border-border hover:bg-muted/40 hover:text-foreground",
                    )}
                    onclick={() => patchFilter(filter.id, { value: opt.value })}
                    >{opt.label}</button
                  >
                {/each}
              </div>
            {:else if colKind === "date"}
              <DateFilterControl
                op={filter.op}
                value={filter.value}
                onchange={(d) => patchFilter(filter.id, { op: /** @type {FilterOp} */ (d.op), value: d.value })}
              />
            {:else if colKind === "number"}
              <Input
                type="text"
                inputmode="decimal"
                data-filter-value
                class={cn(fValue, "font-mono")}
                value={filter.value}
                placeholder="Number…"
                oninput={(e) => {
                  const raw = sanitizeNumericStr(e.currentTarget.value)
                  e.currentTarget.value = raw
                  patchFilter(filter.id, { value: raw })
                }}
              />
            {:else if enumOpts}
              <SearchableMenu
                items={enumOpts.map((v) => ({ value: v, label: v }))}
                placeholder="Search values…"
                contentClass="w-56"
                onselect={(it) => patchFilter(filter.id, { value: it.value })}
              >
                {#snippet trigger(props)}
                  <button
                    {...props}
                    type="button"
                    class={cn(fTrigger, "min-w-[9rem] max-w-[22rem] flex-1")}
                    data-filter-value
                    aria-label="Filter value"
                  >
                    <span class={cn("min-w-0 flex-1 truncate text-left font-mono", !filter.value && "font-sans text-muted-foreground")}>
                      {filter.value || "Select value…"}
                    </span>
                    <Icon name="chevron-down" class="size-3 shrink-0 opacity-50" />
                  </button>
                {/snippet}
                {#snippet item(it)}
                  <span class="min-w-0 flex-1 truncate font-mono">{it.label}</span>
                  {#if filter.value === it.value}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
                {/snippet}
              </SearchableMenu>
            {:else}
              <Input
                data-filter-value
                class={fValue}
                value={filter.value}
                placeholder="Value…"
                oninput={(e) =>
                  patchFilter(filter.id, { value: e.currentTarget.value })}
              />
            {/if}
          {:else}
            <div class="flex-1"></div>
          {/if}
        </div>
      {/each}
      <div class="flex items-center gap-1 px-3 py-1.5">
        <button
          type="button"
          class="inline-flex h-7 items-center gap-1 rounded-md px-2.5 text-ui-sm text-muted-foreground transition-[color,background-color,transform] hover:bg-accent hover:text-foreground active:scale-[0.97]"
          onclick={addFilterAndFocus}
          title="Add a filter condition ({KEY.filter} opens this bar)"
        >
          <Icon name="plus" class="size-3.5" />
          Add filter
        </button>
        <div class="flex-1"></div>
        {#if rowFilters.length > 0}
          <button
            type="button"
            class="inline-flex h-7 items-center gap-1 rounded-md px-2.5 text-ui-sm text-muted-foreground transition-[color,background-color,transform] hover:bg-accent hover:text-foreground active:scale-[0.97]"
            onclick={clearFilters}
            title="Remove every filter condition ({KEY.reset} also clears search, sort and hidden columns)"
          >
            Clear filters
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>
