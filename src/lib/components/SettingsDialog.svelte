<script>
  import { errorText } from '$lib/utils.js'
  import { tick, untrack } from "svelte";
  import { toastPosition, TOAST_POSITIONS } from '$lib/stores/toast-position.js'
  import ThemeBuilder from './ThemeBuilder.svelte';
  import AddThemeDialog from './AddThemeDialog.svelte';
  import { detectOs } from '$lib/platform.js';
  import { importedThemes, activeImportedId, removeImportedTheme, applyImportedTheme } from '$lib/stores/imported-themes.js';
  import { comboTitle } from '$lib/shortcuts.js'
  import Kbd from './Kbd.svelte'
  import { trackInstalled, trackInstall, trackOpen } from '$lib/api.js'
  import { updateState, updateActions } from '$lib/stores/update-state.js'
  import KeybindingsSettings from './KeybindingsSettings.svelte'
  import SnippetsSettings from './SnippetsSettings.svelte'
  import { bindingFor } from '$lib/stores/keybindings.js'
  import { startTelemetry, stopTelemetry } from "$lib/telemetry.js";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import ThemeSwatch from "$lib/components/ThemeSwatch.svelte";
  import ThemeSwatchStrip from "$lib/components/ThemeSwatchStrip.svelte";
  import SearchableMenu from "$lib/components/SearchableMenu.svelte";
  import SelectMenu from "$lib/components/SelectMenu.svelte";
  import SearchField from "./SearchField.svelte";
  import EditorThemePicker from "$lib/components/EditorThemePicker.svelte";
  import { getThemeDefinition, themesByGroup } from "$lib/themes/registry.js";
  import { sidebarSideStore, setSidebarSide } from "$lib/stores/layout.js";
  import { pluginState, isPluginEnabled, setPluginEnabled } from "$lib/stores/plugins.js";
  import { t, locale, LOCALES, setLocale } from "$lib/i18n.js";
  import { licenseStatus } from "$lib/stores/license.js";
  import AccountCard from "./AccountCard.svelte";
  import SyncSection from "./SyncSection.svelte";
  import { isSignedIn, signInWithToast } from "$lib/stores/account.js";
  import {
    appThemeId,
    loadSettings,
    updateSettings,
    systemPreferredTheme,
    increaseZoom,
    decreaseZoom,
    resetZoom,
    canIncreaseZoom,
    canDecreaseZoom,
    FONT_PRESETS,
    DEFAULT_FONT,
    ICON_STYLES,
    ICON_SETS,
    TABLE_STYLES,
    JSON_THEMES,
    TABLE_ALIGN_OPTIONS,
    ROW_SPACINGS,
    GRID_FONT_MIN,
    GRID_FONT_MAX,
    DEFAULT_GRID_FONT_SIZE,
    MOTION_MODES,
    DEFAULT_MAX_QUERY_HISTORY,
    DEFAULT_SESSION_TIMEZONE,
    AGENT_FONT_SIZES,
    THINKING_STYLES,
    settingsRevision,
  } from "$lib/stores/settings.js";
  import { PAGE_SIZE_OPTIONS, loadDefaultPageSize, saveDefaultPageSize } from '$lib/table-query.js';
  import {
    SQL_CASE_OPTIONS,
    SQL_FORMAT_FIELDS,
    SQL_TAB_WIDTHS,
    normalizeSqlFormat,
  } from "$lib/sql-format-options.js";
  import { SQL_EDITOR_FIELDS, SQL_EDITOR_TEXT_SIZES, normalizeSqlEditor } from "$lib/sql-editor-options.js";
  import { aiProfiles, activeProfileId, setActiveProfile } from "$lib/stores/ai-settings.js";
  import PenTool from "@lucide/svelte/icons/pen-tool";
  import LucideSparkles from "@lucide/svelte/icons/sparkles";
  import { HugeiconsIcon } from "@hugeicons/svelte";
  import { SparklesIcon } from "@hugeicons/core-free-icons";
  import PhosphorSparkle from "phosphor-svelte/lib/Sparkle";
  import Icon from "./Icon.svelte";
  import { cn } from "$lib/utils.js";
  import {
    enableAutostart,
    disableAutostart,
    getAutostartStatus,
  } from "$lib/api.js";
  import PinSetupDialog from "./PinSetupDialog.svelte";
  import { treeConnectionScope } from "$lib/stores/sidebar-tree.js";
  import { SHARE_EXPIRIES, loadShareDefaults, saveShareDefaults, openSharesPage } from "$lib/share-flow.js";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import {
    lockStatus,
    refreshLockStatus,
    setLockPrefs,
    lockNow,
  } from "$lib/stores/app-lock.js";

  let {
    open = $bindable(false),
    onopenmcp = () => {},
    onopenmodelconfiguration = () => {},
    onopenabout = () => {},
    onopenextensions = () => {},
    onopenlicense = () => {},
    /** Open on this category instead of General (consumed on open). */
    section = $bindable(''),
  } = $props();

  let settings = $state(loadSettings());
  let shareDefaults = $state(loadShareDefaults());
  /** The search results' container, and whether the query matched nothing. */
  let resultsEl = $state(/** @type {HTMLElement | null} */ (null));
  let noResults = $state(false);
  $effect(() => {
    void q;
    const el = resultsEl;
    if (!el) { noResults = false; return; }
    // Rows render as elements; a query that hides every row leaves only
    // comments and empty text behind.
    tick().then(() => { noResults = el.isConnected && el.childElementCount === 0; });
  });
  /** @param {Partial<import('$lib/share-flow.js').ShareDefaults>} patch */
  function setShareDefault(patch) { shareDefaults = saveShareDefaults(patch); }

  // ── Category navigation + search ─────────────────────────────────────────
  const CATEGORIES = [
    { id: 'general',      label: 'General',      icon: 'sliders-horizontal' },
    { id: 'database',     label: 'Database',     icon: 'database' },
    { id: 'appearance',   label: 'Appearance',   icon: 'sparkles' },
    { id: 'agent',        label: 'Agent',        icon: 'bot' },
    { id: 'account',      label: 'Account & Sync', icon: 'cloud' },
    { id: 'integrations', label: 'Integrations', icon: 'blocks' },
    { id: 'keybindings',  label: 'Keybindings',  icon: 'keyboard' },
    { id: 'snippets',     label: 'Snippets',     icon: 'braces' },
    { id: 'labs',         label: 'Labs',         icon: 'flask-conical' },
    { id: 'about',        label: 'About',        icon: 'info' },
  ];
  let category = $state('general');
  let query = $state('');
  /** The search field has focus: its `/` hint steps aside. */
  let settingsSearchFocused = $state(false);
  const q = $derived(query.trim().toLowerCase());
  const searching = $derived(q.length > 0);
  /** Row visibility under the current search query. */
  // Every word, in any order: "page size" has to find "Rows per page ... page
  // size", and "theme dark" the dark theme row.
  const qWords = $derived(q.split(/\s+/).filter(Boolean));
  const show = (/** @type {string} */ title, /** @type {string} */ desc = '') => {
    if (!q) return true;
    const hay = `${title} ${desc}`.toLowerCase();
    return qWords.every((w) => hay.includes(w));
  };
  const activeCategory = $derived(CATEGORIES.find((c) => c.id === category) ?? CATEGORIES[0]);
  // A caller asking for a page (the command palette's "Keyboard shortcuts")
  // sets `section` and opens the dialog programmatically, which may not go
  // through onOpenChange.
  // Closing forgets the search, however the dialog closed (Escape, the close
  // button, or a caller setting `open`): reopening shows the full page, not
  // the last query's results.
  $effect(() => {
    if (!open) query = '';
  });
  $effect(() => {
    if (open && section) { category = section; query = ''; section = ''; }
  });

  const themeGroups = $derived(themesByGroup());
  /** Kept in sync with applySettings / ⌘M via appThemeId store */
  const activeTheme = $derived(getThemeDefinition($appThemeId));

  $effect(() => {
    const id = $appThemeId;
    if (settings.theme !== id) {
      settings = { ...settings, theme: id };
    }
  });

  const rowCls ="flex items-center justify-between gap-6 border-t border-border/25 py-3.5 first:border-t-0";

  function refreshSettings() {
    settings = loadSettings();
  }

  // ── App PIN ──────────────────────────────────────────────────────────────
  let pinDialogOpen = $state(false);
  /** @type {'set' | 'change' | 'remove'} */
  let pinDialogMode = $state('set');

  /** @param {'set' | 'change' | 'remove'} mode */
  function openPinDialog(mode) {
    pinDialogMode = mode;
    // Settings steps aside first, matching every other "open the real thing"
    // row here - two stacked dialogs fight over the focus trap, and the PIN
    // field must have it.
    open = false;
    pinDialogOpen = true;
  }

  const AUTO_LOCK_OPTIONS = [
    { value: 0, label: 'Never' },
    { value: 1, label: '1 min' },
    { value: 5, label: '5 min' },
    { value: 15, label: '15 min' },
    { value: 60, label: '1 hr' },
  ];

  /** @param {{ requireOnConnect?: boolean, autoLockMinutes?: number }} prefs */
  async function updateLockPrefs(prefs) {
    try {
      await setLockPrefs(prefs);
    } catch (e) {
      toast.error('Could not save the lock setting', { description: errorText(e) });
    }
  }

  /** @param {boolean} next */
  // Reload the copy whenever the settings change elsewhere (⌘K, a menu, sync)
  // and whenever the dialog opens. onOpenChange alone missed both: bits-ui
  // does not call it when `open` is set from code, as the palette does.
  $effect(() => {
    void $settingsRevision;
    if (open) untrack(() => refreshSettings());
  });

  function handleOpenChange(next) {
    if (next) { refreshSettings(); category = section || 'general'; section = ''; query = ''; void refreshLockStatus(); }
  }

  /** @param {import('$lib/themes/registry.js').ThemeId} themeId */
  let themeBuilderOpen = $state(false);
  let addThemeOpen = $state(false);
  const isMacOs = detectOs() === 'macos';

  /** Use one of the imported themes. @param {string} id */
  function useImported(id) {
    activeImportedId.set(id);
    if ($appThemeId === 'imported') applyImportedTheme();
    else settings = updateSettings({ theme: 'imported' });
  }

  /** @param {string} id */
  function dropImported(id) {
    removeImportedTheme(id);
    if ($appThemeId === 'imported') {
      if ($importedThemes.length) applyImportedTheme();
      else settings = updateSettings({ theme: systemPreferredTheme() });
    }
  }

  function setTheme(themeId) {
    if (themeId === $appThemeId) return;
    settings = updateSettings({ theme: themeId });
  }

  function bumpZoom(delta) {
    settings = delta > 0 ? increaseZoom() : decreaseZoom();
  }

  /** @param {import('$lib/stores/settings.js').FontId} font */
  function setFont(font) {
    if (font === settings.font) return;
    settings = updateSettings({ font });
  }
  // The default face leads the list (SF Pro on a Mac, Geist elsewhere).
  const fontEntries = Object.entries(FONT_PRESETS).sort(([a], [b]) => Number(b === DEFAULT_FONT) - Number(a === DEFAULT_FONT));
  const iconStyleEntries = Object.entries(ICON_STYLES);
  const iconSetEntries = Object.entries(ICON_SETS);
  const tableStyleEntries = Object.entries(TABLE_STYLES);
  const jsonThemeEntries = Object.entries(JSON_THEMES);
  /** @param {string | undefined} id */
  function setJsonTheme(id) {
    if (!id || id === settings.jsonTheme) return;
    settings = updateSettings({ jsonTheme: /** @type {any} */ (id) });
  }
  /** @param {import('$lib/themes/editor-themes.js').EditorThemeId} id */
  function setEditorTheme(id) {
    if (id === settings.editorTheme) return;
    settings = updateSettings({ editorTheme: id });
  }
  // Theme-aware CSS previews (mirror how each preset renders on the canvas grid).
  const tableStylePreview = {
    lines:   "background-image:linear-gradient(var(--border) 1px,transparent 1px),linear-gradient(90deg,var(--border) 1px,transparent 1px);background-size:7px 7px;",
    bordered:"background-image:linear-gradient(var(--muted-foreground) 1px,transparent 1px),linear-gradient(90deg,var(--muted-foreground) 1px,transparent 1px);background-size:7px 7px;",
    striped: "background-image:repeating-linear-gradient(var(--muted) 0 7px,transparent 7px 14px);",
    dotted:  "background-image:radial-gradient(color-mix(in oklab,var(--border) 90%,transparent) 0.7px,transparent 0.8px);background-size:4px 4px;",
    dots:    "background-image:radial-gradient(var(--muted-foreground) 1.1px,transparent 1.3px);background-size:9px 9px;background-position:center;",
    minimal: "background-image:linear-gradient(var(--border) 1px,transparent 1px);background-size:100% 7px;",
    dashed:  "background-image:repeating-linear-gradient(90deg,var(--border) 0 3px,transparent 3px 6px),repeating-linear-gradient(0deg,var(--border) 0 3px,transparent 3px 6px);background-size:100% 7px,7px 100%;background-repeat:repeat;",
    columns: "background-image:linear-gradient(90deg,var(--border) 1px,transparent 1px);background-size:7px 100%;",
  };

  /** @param {string} v */
  function setTableAlign(v) {
    if (v) settings = updateSettings({ tableTextAlign: v });
  }
  /** @param {import('$lib/stores/settings.js').TableStyleId} tableStyle */
  function setTableStyle(tableStyle) {
    if (tableStyle === settings.tableStyle) return;
    settings = updateSettings({ tableStyle });
  }

  /** @param {import('$lib/stores/settings.js').IconStyleId} iconStyle */
  function setIconStyle(iconStyle) {
    if (iconStyle === settings.iconStyle) return;
    settings = updateSettings({ iconStyle });
  }

  /** @param {import('$lib/stores/settings.js').IconSetId} iconSet */
  function setIconSet(iconSet) {
    if (iconSet === settings.iconSet) return;
    settings = updateSettings({ iconSet });
  }

  function toggleMcpAutoStart() {
    settings = updateSettings({ mcpAutoStart: !settings.mcpAutoStart });
  }

  function toggleAutoReconnect() {
    settings = updateSettings({ autoReconnectOnStartup: !settings.autoReconnectOnStartup });
  }

  function togglePreviewDml() {
    settings = updateSettings({ previewDmlBeforeApply: !settings.previewDmlBeforeApply });
  }

  function toggleVimMode() {
    settings = updateSettings({ vimMode: !settings.vimMode });
  }

  function toggleJsonWordWrap() {
    settings = updateSettings({ jsonWordWrap: !settings.jsonWordWrap });
  }

  function toggleNativeScroll() {
    settings = updateSettings({ nativeScroll: !settings.nativeScroll });
  }

  const rowSpacingEntries = Object.entries(ROW_SPACINGS);
  // Rows-per-page is owned by table-query.js, not by this settings blob - the
  // grid's own page-size dropdown writes it. Mirrored into local state so the
  // row re-renders after a write, since there is no store to subscribe to.
  let defaultPageSize = $state(loadDefaultPageSize());
  const pageSizeItems = PAGE_SIZE_OPTIONS
    .filter((n) => n > 0 && n <= 1_000)
    .map((n) => ({ value: String(n), label: `${n} rows`, keywords: [String(n)] }));
  const motionEntries = Object.entries(MOTION_MODES);
  // NULL rendering is already an extension ("Empty & NULL Markers"), and a
  // second implementation in Settings would be two switches for one behaviour.
  // What was missing is discoverability: nobody goes looking in Extensions for
  // how NULL is drawn. Same extension, surfaced where people look for it.
  const NULLISH_ID = 'nullish-values';
  const nullishOn = $derived.by(() => { void $pluginState; return isPluginEnabled(NULLISH_ID); });
  // Same story as NULL: "Boolean Glyphs" is a per-cell formatter, and formatters
  // run after the grid's own value formatting and replace it. A boolean-display
  // setting beside this toggle would be dead whenever the extension was on.
  const BOOL_GLYPH_ID = 'boolean-glyph';
  const boolGlyphOn = $derived.by(() => { void $pluginState; return isPluginEnabled(BOOL_GLYPH_ID); });

  const sidebarSideItems = [
    { value: 'left', label: 'Left' },
    { value: 'right', label: 'Right' },
  ];

  /** @param {string | undefined} id */
  function setMotion(id) {
    if (!id || id === settings.motion) return;
    settings = updateSettings({ motion: /** @type {any} */ (id) });
  }

  /** @param {string | undefined} id */
  function setRowSpacing(id) {
    if (!id || id === settings.rowSpacing) return;
    settings = updateSettings({ rowSpacing: /** @type {any} */ (id) });
  }

  function toggleZebraRows() {
    settings = updateSettings({ zebraRows: !settings.zebraRows });
  }

  function toggleRowNumbers() {
    settings = updateSettings({ showRowNumbers: !settings.showRowNumbers });
  }
  function toggleRelationColumns() {
    settings = updateSettings({ showRelationColumns: settings.showRelationColumns === false });
  }

  function toggleMenuBar() {
    settings = updateSettings({ showMenuBar: !settings.showMenuBar });
  }

  function toggleNumberGrouping() {
    settings = updateSettings({ numberGrouping: !settings.numberGrouping });
  }

  function toggleImagePreview() {
    settings = updateSettings({ imagePreview: !settings.imagePreview });
  }

  function toggleFkAutoExpandJson() {
    settings = updateSettings({ fkAutoExpandJson: !settings.fkAutoExpandJson });
  }

  function toggleOpenUrls() {
    settings = updateSettings({ openUrlsOnClick: !settings.openUrlsOnClick });
  }

  function toggleHighlightActiveRow() {
    settings = updateSettings({ highlightActiveRow: !settings.highlightActiveRow });
  }

  /** @param {number} px */
  function setGridFontSize(px) {
    const next = Math.min(GRID_FONT_MAX, Math.max(GRID_FONT_MIN, Math.round(px)));
    if (next === settings.gridFontSize) return;
    settings = updateSettings({ gridFontSize: next });
  }

  /** @param {string | undefined} v */
  function setDefaultPageSize(v) {
    const n = Number(v);
    if (!Number.isFinite(n) || n === defaultPageSize) return;
    saveDefaultPageSize(n);
    defaultPageSize = loadDefaultPageSize();
  }

  function toggleAutoSaveQueries() {
    settings = updateSettings({ autoSaveQueries: !settings.autoSaveQueries });
  }

  function toggleStreamResults() {
    settings = updateSettings({ streamResults: settings.streamResults === false });
  }

  function toggleSqlUndo() {
    settings = updateSettings({ sqlUndo: settings.sqlUndo === false });
  }

  // ── SQL editor ────────────────────────────────────────────────────────────
  // Read through the normalizer for the same reason as sqlFmt below.
  const sqlEd = $derived(/** @type {any} */ (normalizeSqlEditor(settings.sqlEditor)));
  /** @param {string} key @param {unknown} value */
  function setSqlEditor(key, value) {
    settings = updateSettings({ sqlEditor: normalizeSqlEditor({ ...sqlEd, [key]: value }) });
  }

  // ── SQL formatting ────────────────────────────────────────────────────────
  // Collapsed by default - see the section comment in databaseContent.
  let sqlFmtOpen = $state(false);
  // Read through the normalizer, never straight off `settings`. Settings persisted
  // by a build that predates this option set can be partial (or absent), and one
  // `undefined[key]` in a row would throw the whole pane into the error boundary.
  const sqlFmt = $derived(/** @type {any} */ (normalizeSqlFormat(settings.sqlFormat)));

  /** @param {string} key @param {unknown} value */
  function setSqlFormat(key, value) {
    const next = { ...sqlFmt, [key]: value };
    settings = updateSettings({ sqlFormat: normalizeSqlFormat(next) });
  }
  /** @param {{ key: string, min?: number, max?: number, step?: number }} field @param {number} dir */
  function bumpSqlFormat(field, dir) {
    const step = field.step ?? 1;
    const cur = Number(sqlFmt[field.key]);
    const next = Math.min(field.max ?? 999, Math.max(field.min ?? 0, cur + dir * step));
    if (next !== cur) setSqlFormat(field.key, next);
  }

  function toggleCmdkAi() {
    settings = updateSettings({ cmdkAiEnabled: !settings.cmdkAiEnabled });
  }

  function toggleAiInTab() {
    settings = updateSettings({ aiInTab: !settings.aiInTab });
  }
  function toggleTreeSidebar() {
    settings = updateSettings({ sidebarStyle: settings.sidebarStyle === 'tree' ? 'classic' : 'tree' });
  }
  function toggleLiveMode() {
    settings = updateSettings({ liveModeEnabled: !settings.liveModeEnabled });
  }

  function toggleLazyWideColumns() {
    settings = updateSettings({ lazyWideColumns: !settings.lazyWideColumns });
  }

  // ── Database (query & connection) numeric/text settings ──────────────────
  /** @param {keyof import('$lib/stores/settings.js').AppSettings} key @param {string|number} raw @param {number} def @param {number} min */
  function setNumber(key, raw, def, min = 0) {
    let n = Math.round(Number(raw));
    if (!Number.isFinite(n)) n = def;
    if (n < min) n = min;
    settings = updateSettings({ [key]: n });
  }
  /** @param {keyof import('$lib/stores/settings.js').AppSettings} key @param {string} raw @param {string} def */
  function setText(key, raw, def) {
    settings = updateSettings({ [key]: String(raw).trim() || def });
  }
  /** @param {keyof import('$lib/stores/settings.js').AppSettings} key @param {string|number} def */
  function resetField(key, def) {
    settings = updateSettings({ [key]: def });
  }

  const DATA_VIEW_OPTIONS = [
    { id: 'table',  label: 'Table',  icon: 'table-2' },
    { id: 'json',   label: 'JSON',   icon: 'braces' },
    { id: 'record', label: 'Record', icon: 'layout-list' },
    { id: 'text',   label: 'Text',   icon: 'file-text' },
    { id: 'chart',  label: 'Chart',  icon: 'bar-chart-2' },
    { id: 'erd',    label: 'Schema diagram', icon: 'network' },
  ];
  const defaultViewOption = $derived(
    DATA_VIEW_OPTIONS.find((o) => o.id === settings.defaultDataView) ?? DATA_VIEW_OPTIONS[0],
  );
  /** @param {string} v */
  function setDefaultDataView(v) {
    if (v) settings = updateSettings({ defaultDataView: v });
  }

  const PAGINATION_OPTIONS = [
    { id: 'offset',   label: 'Offset',   icon: 'hash', hint: 'Classic LIMIT/OFFSET, jump to any page' },
    { id: 'cursor',   label: 'Cursor',   icon: 'chevrons-right', hint: 'Keyset by primary key: fast next/prev, no page jump' },
    { id: 'keyset',   label: 'Keyset',   icon: 'key-round', hint: 'Same as cursor (keyset on the primary key)' },
    { id: 'temporal', label: 'Temporal', icon: 'clock', hint: 'Keyset on a timestamp column, newest-first' },
  ];
  /** @param {string} v */
  function setPaginationMode(v) {
    if (v) settings = updateSettings({ paginationMode: v });
  }

  const NULL_SORT_OPTIONS = [
    { id: 'unset', label: 'Unset' },
    { id: 'first', label: 'Nulls First' },
    { id: 'last', label: 'Nulls Last' },
  ];
  const nullSortOption = $derived(NULL_SORT_OPTIONS.find((o) => o.id === settings.nullSortOrder) ?? NULL_SORT_OPTIONS[0]);
  /** @param {string} v */
  function setNullSort(v) {
    if (v) settings = updateSettings({ nullSortOrder: v });
  }

  // ── Agent (AI chat) settings ─────────────────────────────────────────────
  /** @param {string} v */
  function setAgentChatFont(v) {
    const n = Number(v);
    if (Number.isFinite(n)) settings = updateSettings({ agentChatFontSize: n });
  }
  /** @param {string} v */
  function setAgentCodeFont(v) {
    const n = Number(v);
    if (Number.isFinite(n)) settings = updateSettings({ agentCodeFontSize: n });
  }
  /** @param {string} v */
  function setAgentThinking(v) {
    if (v) settings = updateSettings({ agentThinkingStyle: v });
  }
  function toggleAgentQueryCards() {
    settings = updateSettings({ agentShowQueryCards: !settings.agentShowQueryCards });
  }
  function toggleAgentWebAccess() {
    settings = updateSettings({ agentWebAccess: !settings.agentWebAccess });
  }
  function toggleAutoUpdate() {
    settings = updateSettings({ autoUpdate: !settings.autoUpdate });
  }
  function toggleTelemetry() {
    settings = updateSettings({ telemetry: !settings.telemetry });
    // Takes effect immediately rather than at next launch: a privacy switch
    // that needs a restart to mean anything is not a privacy switch.
    if (!settings.telemetry) stopTelemetry();
    else startTelemetry();
  }
  const thinkingStyleOption = $derived(
    THINKING_STYLES.find((s) => s.id === settings.agentThinkingStyle) ?? THINKING_STYLES[0],
  );
  const activeModelProfile = $derived(
    $aiProfiles.find((p) => p.id === $activeProfileId) ?? $aiProfiles[0],
  );
  /** @param {string} v */
  function setModelProfile(v) {
    if (v) setActiveProfile(v);
  }

  /** @type {boolean | null} */
  let launchAtLogin = $state(null);

  // Asked on first open, not at startup: on macOS this shells out to
  // AppleScript (~300 ms) and the dialog is mounted for the app's lifetime.
  $effect(() => {
    if (open && launchAtLogin === null) {
      getAutostartStatus()
        .then((v) => { launchAtLogin = v; })
        .catch(() => { launchAtLogin = false; });
    }
  });

  async function toggleLaunchAtLogin() {
    const next = !launchAtLogin;
    launchAtLogin = next;
    try {
      if (next) {
        await enableAutostart();
      } else {
        await disableAutostart();
      }
    } catch {
      launchAtLogin = !next;
    }
  }

  function openModelConfiguration() {
    open = false;
    onopenmodelconfiguration();
  }

  const zoomLabel = $derived(`${Math.round(settings.zoom * 100)}%`);

  const planBadge = $derived.by(() => {
    const s = $licenseStatus;
    if (s?.status === "Valid") return { label: "Pro", class: "chip-gloss" };
    if (s?.status === "Trial") return { label: `Trial · ${s.days_remaining}d`, class: "chip-gloss chip-warning" };
    return { label: "Free", class: "chip-gloss chip-muted" };
  });

  // ── Updates: version and track ─────────────────────────────────────
  // Stable and Nightly are separate apps (own name and data), so the track is
  // which app runs: switching installs the other one if needed, opens it and
  // quits this one.
  let appVersion = $state('')
  const onNightly = $derived(/-nightly\./.test(appVersion))
  $effect(() => {
    void import('@tauri-apps/api/app')
      .then((m) => m.getVersion())
      .then((v) => { appVersion = v })
      .catch(() => {})
  })

  /** A check asked for from here came back with nothing: say so on the button. */
  let upToDate = $state(false)
  let askedCheck = $state(false)
  $effect(() => {
    const u = $updateState
    if (!askedCheck || u.checking) return
    upToDate = u.status === 'idle'
    askedCheck = false
  })
  function checkUpdates() {
    upToDate = false
    askedCheck = true
    updateActions.check()
  }

  /** @type {import('$lib/api.js').UpdateTrack | null} */
  let switchTo = $state(null)
  let switchInstalled = $state(/** @type {string | null} */ (null))
  let switchBusy = $state(false)
  let switchError = $state('')
  const switchName = $derived(switchTo === 'nightly' ? 'Stroke Nightly' : 'Stroke')

  /** @param {import('$lib/api.js').UpdateTrack} track */
  async function askSwitch(track) {
    if (track === (onNightly ? 'nightly' : 'stable')) return
    switchError = ''
    switchInstalled = null
    switchTo = track
    try { switchInstalled = await trackInstalled(track) } catch { /* browser dev */ }
  }

  async function confirmSwitch() {
    if (!switchTo) return
    const track = switchTo
    switchBusy = true
    switchError = ''
    try {
      const path = switchInstalled ?? (await trackInstall(track))
      await trackOpen(track, path)
      switchTo = null
      // The ordinary quit: unsaved edits still get asked about.
      const { emit } = await import('@tauri-apps/api/event')
      await emit('app-quit-requested')
    } catch (e) {
      switchError = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      switchBusy = false
    }
  }
</script>

<Dialog.Root bind:open onOpenChange={handleOpenChange}>
  <Dialog.Content motion="fade" class="flex h-[min(40rem,88vh)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[56rem]">
    <Dialog.Title class="sr-only">Settings</Dialog.Title>
    <Dialog.Description class="sr-only">Appearance, behavior, and integrations</Dialog.Description>

    <div class="grid min-h-0 flex-1 grid-cols-[13rem_minmax(0,1fr)] overflow-hidden">
      <!-- ── Left: search + category nav ─────────────────────────── -->
      <aside class="flex min-h-0 flex-col gap-3 border-r border-border/40 bg-muted/[0.015] p-3">
        <SearchField bind:value={query} placeholder="Search" label="Search settings" rank={30} />
        <!-- Scrolls when zoomed: at 150% in a small window the last sections
             (Labs, About) sat below the dialog with no way to reach them. -->
        <nav class="app-scroll -mx-1 flex min-h-0 flex-col gap-0.5 overflow-y-auto overscroll-contain px-1">
          {#each CATEGORIES as c (c.id)}
            <button
              type="button"
              onclick={() => { category = c.id; query = ''; }}
              class={cn(
                'flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-ui-sm transition-[background-color,color,transform] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] active:scale-[0.98]',
                !searching && category === c.id
                  ? 'bg-muted/60 font-medium text-foreground'
                  : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground',
              )}
            >
              <Icon name={c.icon} class="size-4 shrink-0" />
              {$t('settings.nav.' + c.id)}
            </button>
          {/each}
        </nav>
      </aside>

      <!-- ── Right: content ──────────────────────────────────────── -->
      <div class="app-scroll min-h-0 overflow-y-auto">
        <div class="mx-auto max-w-[42rem] px-8 py-7">
          {#key searching ? '__search__' : category}
            <div class="settings-pane">
              <h2 class="mb-6 text-ui-lg font-semibold tracking-tight text-foreground">
                {searching ? `Results for "${query.trim()}"` : $t('settings.nav.' + activeCategory.id)}
              </h2>

              {#if searching}
                <div bind:this={resultsEl}>
                  {@render generalContent()}
                  {@render databaseContent()}
                  {@render appearanceContent()}
                  {@render agentContent()}
                  {@render accountContent()}
                  {@render integrationsContent()}
                  {@render labsContent()}
                  {@render aboutContent()}
                </div>
                {#if noResults}
                  <div class="flex flex-col items-center gap-2 py-16 text-center">
                    <Icon name="search" class="size-5 shrink-0 text-muted-foreground" />
                    <p class="text-ui-sm font-medium text-foreground">No settings match</p>
                    <p class="text-ui-xs text-muted-foreground">Try another word, like "font", "theme" or "share".</p>
                  </div>
                {/if}
              {:else if category === 'general'}
                {@render generalContent()}
              {:else if category === 'database'}
                {@render databaseContent()}
              {:else if category === 'appearance'}
                {@render appearanceContent()}
              {:else if category === 'agent'}
                {@render agentContent()}
              {:else if category === 'keybindings'}
                <KeybindingsSettings />
              {:else if category === 'snippets'}
                <SnippetsSettings />
              {:else if category === 'account'}
                {@render accountContent()}
              {:else if category === 'integrations'}
                {@render integrationsContent()}
              {:else if category === 'labs'}
                {@render labsContent()}
              {:else}
                {@render aboutContent()}
              {/if}
            </div>
          {/key}
        </div>
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>

<PinSetupDialog bind:open={pinDialogOpen} mode={pinDialogMode} />

<!-- ── Content snippets ──────────────────────────────────────────── -->
<!-- Labs: features still being shaped. Each was a "(beta)" or "(experimental)"
     switch scattered through General; they live together here, so what is
     settled and what is still moving read as two different things. -->
{#snippet labsContent()}
  {#if !searching}
    <p class="mb-4 text-ui-xs leading-relaxed text-pretty text-muted-foreground">
      Features still being shaped. They work, they may change, and the ones that settle move to their own page.
    </p>
  {/if}
  {#if show('AI chat in a tab', 'Labs, open AI chat as a tab beside your tables, with its chats in the sidebar')}
    {@render switchRow('AI chat in a tab', 'AI chat opens as a tab beside your tables and queries, with its chats in the sidebar, instead of taking over the whole window.', settings.aiInTab, toggleAiInTab)}
  {/if}
  {#if show('Tree sidebar', 'Labs, browse connections, schemas, tables and columns as one explorer tree')}
    {@render switchRow('Tree sidebar', 'One explorer tree in the sidebar: connection, database, schema, tables, views and routines, columns. Replaces the Tables, Objects and Databases tabs; Queries, Recent and Find & replace stay.', settings.sidebarStyle === 'tree', toggleTreeSidebar)}
    {#if settings.sidebarStyle === 'tree'}
      <div class={rowCls}>
        <div class="min-w-0">
          <p class="text-ui-sm font-medium text-foreground">Connections in the tree</p>
          <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Every saved connection as a root, or only the connected one with the rest in a Connections list below it.</p>
        </div>
        {@render segmented('Connections in the tree', [{ value: 'all', label: 'All' }, { value: 'connected', label: 'Connected only' }], $treeConnectionScope, (v) => treeConnectionScope.set(v))}
      </div>
    {/if}
  {/if}
  {#if show('Several connections at once', 'Labs, multiple connections open together, switch without closing tabs')}
    {@render switchRow(
      'Several connections at once',
      'Switching keeps the connection you leave open, with its tabs, and switching back is instant. Up to six stay open; connections through an SSH tunnel still reconnect.',
      settings.multiConnection === true,
      () => {
        settings = updateSettings({ multiConnection: settings.multiConnection !== true });
        // The shell closes the kept databases and drops them from the tab bar
        // and the status bar; only it knows which ones are open.
        if (!settings.multiConnection) window.dispatchEvent(new CustomEvent('stroke:multi-connection-off'));
      },
    )}
  {/if}
  {#if show('Share as a web studio', 'Labs, share a connection with a link that opens in any browser, relay, remote')}
    {@render switchRow(
      'Share as a web studio',
      'Share the open database with a link that works in any browser, on any device, while Stroke is open. End-to-end encrypted through stroke.click. Read-only unless you allow edits.',
      settings.studioShare === true,
      () => {
        settings = updateSettings({ studioShare: settings.studioShare !== true });
        window.dispatchEvent(new CustomEvent('stroke:studio-share-changed', { detail: settings.studioShare === true }));
      },
    )}
  {/if}
  {#if show('Focus the tree on switch', 'Labs, collapse the sidebar tree to the new database tables when you switch')}
    {@render switchRow(
      'Focus the tree on switch',
      'Switching to another database folds the tree to that database: its default schema (public, main) open on its tables, everything else closed.',
      settings.treeFocusOnSwitch === true,
      () => { settings = updateSettings({ treeFocusOnSwitch: settings.treeFocusOnSwitch !== true }) },
    )}
  {/if}
  {#if show('Boring connection bar', 'Labs, group saved connections into a folder per database kind')}
    {@render switchRow(
      'Boring connection bar',
      'Saved connections sit in one folder per database kind (PostgreSQL, MySQL, Redis, ...) in the connections list, instead of one long list by last use.',
      settings.boringConnectionBar === true,
      () => { settings = updateSettings({ boringConnectionBar: settings.boringConnectionBar !== true }) },
    )}
  {/if}
  {#if show('Live mode', 'Labs, auto-refresh the active table when its data changes')}
    {@render switchRow('Live mode', 'Shows the Live toggle in the status bar, which refreshes the open table when its data changes.', settings.liveModeEnabled, toggleLiveMode)}
  {/if}
  {#if show('Cmd+K AI', 'Labs, ask AI directly from the command palette')}
    {@render switchRow(`${comboTitle('Mod+K')} AI`, `Shows "Ask AI" in the ${comboTitle('Mod+K')} command palette.`, settings.cmdkAiEnabled, toggleCmdkAi)}
  {/if}
  {#if show('Vim mode', 'Labs, modal keyboard navigation across the app, the data grid, and the SQL editor')}
    {@render switchRow('Vim mode', 'Modal keyboard navigation (hjkl, gg/G, i/Esc) across the grid, the SQL editor and tabs.', settings.vimMode, toggleVimMode)}
  {/if}
{/snippet}

{#snippet secLabel(/** @type {string} */ text)}
  {#if !searching}
    <!-- No rule of its own: the first row's top border is the line under it.
         Both drew one, which is the double line under every heading. -->
    <p class="mt-8 mb-1 text-ui-2xs font-semibold uppercase tracking-[0.06em] text-muted-foreground first:mt-0">{text}</p>
  {/if}
{/snippet}

{#snippet switchRow(/** @type {string} */ label, /** @type {string} */ desc, /** @type {boolean} */ checked, /** @type {() => void} */ ontoggle)}
  <div class={rowCls}>
    <div class="min-w-0">
      <p class="text-ui-sm font-medium text-foreground">{label}</p>
      <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{desc}</p>
    </div>
    <button
      type="button" role="switch" aria-checked={checked} aria-label={label}
      onclick={ontoggle}
      class="app-switch cursor-pointer"
    >
      <span class="app-switch-thumb"></span>
    </button>
  </div>
{/snippet}

{#snippet actionRow(/** @type {string} */ label, /** @type {string} */ desc, /** @type {string} */ btn, /** @type {() => void} */ onclick, /** @type {{label:string,class:string}|null} */ badge = null)}
  <div class={rowCls}>
    <div class="min-w-0">
      <p class="flex items-center gap-2 text-ui-sm font-medium text-foreground">
        {label}
        {#if badge}<span class="h-5 px-1.5 text-ui-3xs font-semibold {badge.class}">{badge.label}</span>{/if}
      </p>
      <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{desc}</p>
    </div>
    <Button type="button" variant="outline" size="sm" class="shrink-0" {onclick}>{btn}</Button>
  </div>
{/snippet}

{#snippet resetBtn(/** @type {string} */ key, /** @type {string|number} */ def, /** @type {boolean} */ dirty)}
  <button
    type="button"
    onclick={() => resetField(/** @type {any} */ (key), def)}
    disabled={!dirty}
    title={$t('settings.resetDefault')}
    aria-label={$t('settings.resetDefault')}
    class={cn(
      'inline-flex size-8 shrink-0 items-center justify-center rounded-lg border transition-[background-color,color,border-color,opacity] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] active:scale-[0.96]',
      dirty
        ? 'border-border/60 text-muted-foreground hover:bg-muted hover:text-foreground'
        : 'cursor-default border-transparent text-muted-foreground',
    )}
  >
    <RotateCcw class="size-3.5 shrink-0" />
  </button>
{/snippet}

{#snippet numberRow(/** @type {string} */ label, /** @type {string} */ desc, /** @type {string} */ key, /** @type {number} */ def, /** @type {string} */ unit, /** @type {number} */ min)}
  <div class={rowCls}>
    <div class="min-w-0">
      <p class="text-ui-sm font-medium text-foreground">{label}</p>
      <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{desc}</p>
    </div>
    <div class="flex shrink-0 items-center gap-1.5">
      <div class="relative">
        <input
          type="number" {min}
          value={settings[key]}
          aria-label={label}
          onchange={(e) => setNumber(/** @type {any} */ (key), e.currentTarget.value, def, min)}
          class={cn(
            'h-8 w-48 rounded-lg border-2 border-border bg-background pl-2.5 text-right font-mono text-ui-xs tabular-nums text-foreground outline-none transition-[border-color,box-shadow] focus:border-ring/55 focus:ring-2 focus:ring-ring/15',
            unit ? 'pr-11' : 'pr-2.5',
          )}
        />
        {#if unit}<span class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-ui-2xs text-muted-foreground">{unit}</span>{/if}
      </div>
      {@render resetBtn(key, def, settings[key] !== def)}
    </div>
  </div>
{/snippet}

{#snippet textRow(/** @type {string} */ label, /** @type {string} */ desc, /** @type {string} */ key, /** @type {string} */ def)}
  <div class={rowCls}>
    <div class="min-w-0">
      <p class="text-ui-sm font-medium text-foreground">{label}</p>
      <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{desc}</p>
    </div>
    <div class="flex shrink-0 items-center gap-1.5">
      <input
        type="text" spellcheck="false" autocapitalize="off" autocomplete="off"
        value={settings[key]}
        aria-label={label}
        onchange={(e) => setText(/** @type {any} */ (key), e.currentTarget.value, def)}
        class= "field-surface h-8 w-48 bg-background px-2.5 font-mono text-ui-xs text-foreground outline-none transition-[border-color,box-shadow]"
      />
      {@render resetBtn(key, def, settings[key] !== def)}
    </div>
  </div>
{/snippet}

{#snippet databaseContent()}
  {@render secLabel($t('settings.sec.dataView'))}
  {#if show($t('settings.defaultView'), $t('settings.defaultView.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.defaultView')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.defaultView.desc')}</p>
      </div>
      <SelectMenu
        ariaLabel="Default view"
        value={settings.defaultDataView}
        onValueChange={setDefaultDataView}
        items={DATA_VIEW_OPTIONS.map((o) => ({ value: o.id, label: o.label, icon: o.icon, keywords: [o.label] }))}
      />
    </div>
  {/if}

  {#if show($t('settings.pagination'), $t('settings.pagination.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.pagination')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.pagination.desc')}</p>
      </div>
      <SelectMenu
        ariaLabel="Pagination strategy"
        value={settings.paginationMode}
        onValueChange={setPaginationMode}
        items={PAGINATION_OPTIONS.map((o) => ({ value: o.id, label: o.label, icon: o.icon, keywords: [o.label] }))}
      />
    </div>
  {/if}

  {@render secLabel('Result Ordering')}
  {#if show('Null sort order', 'Applied to quick-query ordering on databases that support explicit null placement')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Null sort order</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Applies where the database supports explicit null placement.</p>
      </div>
      <SelectMenu
        ariaLabel="Null sort order"
        value={settings.nullSortOrder}
        onValueChange={setNullSort}
        items={NULL_SORT_OPTIONS.map((o) => ({ value: o.id, label: o.label, keywords: [o.label] }))}
      />
    </div>
  {/if}

  {@render secLabel($t('settings.sec.queryHistory'))}
  {#if show($t('settings.maxQueryHistory'), $t('settings.maxQueryHistory.desc'))}
    {@render numberRow($t('settings.maxQueryHistory'), $t('settings.maxQueryHistory.desc'), 'maxQueryHistory', DEFAULT_MAX_QUERY_HISTORY, '', 1)}
  {/if}

  {@render secLabel($t('settings.sec.connectionDefaults'))}
  {#if show($t('settings.timezone'), $t('settings.timezone.desc'))}
    {@render textRow($t('settings.timezone'), $t('settings.timezone.desc'), 'sessionTimezone', DEFAULT_SESSION_TIMEZONE)}
  {/if}

  {#if show('Auto-save executed queries', 'File every successful run under Saved Queries, not just Query History')}
    {@render switchRow(
      'Auto-save executed queries',
      'Keep every successful run in Saved Queries, deduplicated by its SQL.',
      settings.autoSaveQueries,
      toggleAutoSaveQueries,
    )}
  {/if}

  {#if show('Stream query results', 'Keep large results in a file and load the rows you scroll to')}
    {@render switchRow(
      'Stream query results',
      'Keep a result in a file on this machine and load only the rows you scroll to, so millions of rows scroll and switch tabs smoothly. Off loads every row into the window.',
      settings.streamResults !== false,
      toggleStreamResults,
    )}
  {/if}

  {#if show('Revert console writes', 'Keep a copy of the rows an UPDATE, DELETE or INSERT changes so it can be reverted')}
    {@render switchRow(
      'Revert console writes',
      'Keep a copy of the rows a single UPDATE, DELETE or INSERT changes, so its Revert button can put them back. Postgres, MySQL, MariaDB and SQLite, up to 10,000 rows, in memory only.',
      settings.sqlUndo !== false,
      toggleSqlUndo,
    )}
  {/if}

  <!-- SQL formatting: nine options, so it opens COLLAPSED. Mounting nine popover
       selects just to show the Database tab is what made this pane feel slow, and
       these are settings you touch once. Search still reaches them - a query
       expands the section, because a setting you can't find may as well not exist. -->
  <!-- SQL editor: one short row each, so unlike formatting it is not collapsed. -->
  {@render secLabel('SQL editor')}
  {#each SQL_EDITOR_FIELDS as field (field.key)}
    {#if show(field.label, field.desc)}
      {#if field.kind === 'bool'}
        {@render switchRow(field.label, field.desc, sqlEd[field.key] === true, () => setSqlEditor(field.key, !sqlEd[field.key]))}
      {:else}
        <div class={rowCls}>
          <div class="min-w-0">
            <p class="text-ui-sm font-medium text-foreground">{field.label}</p>
            <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{field.desc}</p>
          </div>
          {#if field.kind === 'choice'}
            {@render segmented(field.label, /** @type {any} */ (field).options, sqlEd[field.key], (v) => setSqlEditor(field.key, v))}
          {:else}
            {@render segmented(field.label, SQL_EDITOR_TEXT_SIZES.map((o) => ({ value: o.id, label: o.label })), sqlEd.textSize, (v) => setSqlEditor('textSize', v))}
          {/if}
        </div>
      {/if}
    {/if}
  {/each}

  {@render secLabel('SQL formatting')}
  {#if !searching}
    <button
      type="button"
      class="{rowCls} w-full text-left"
      aria-expanded={sqlFmtOpen}
      onclick={() => (sqlFmtOpen = !sqlFmtOpen)}
    >
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Formatting options</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Casing, indentation and wrapping for Format (⇧⌥F).
        </p>
      </div>
      <Icon name={sqlFmtOpen ? 'chevron-up' : 'chevron-down'} class="size-3.5 shrink-0 text-muted-foreground" />
    </button>
  {/if}
  {#if sqlFmtOpen || searching}
    <!-- One row per sql-formatter option, rendered from SQL_FORMAT_FIELDS so the
         list can't drift from what the formatter actually reads. -->
    {#each SQL_FORMAT_FIELDS as field (field.key)}
      {#if show(field.label, field.desc)}
        {#if field.kind === 'bool'}
          {@render switchRow(field.label, field.desc, sqlFmt[field.key] === true, () => setSqlFormat(field.key, !sqlFmt[field.key]))}
        {:else}
          <div class={rowCls}>
            <div class="min-w-0">
              <p class="text-ui-sm font-medium text-foreground">{field.label}</p>
              <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{field.desc}</p>
            </div>
            {#if field.kind === 'case'}
              {@render segmented(field.label, SQL_CASE_OPTIONS.map((o) => ({ value: o.id, label: o.label })), sqlFmt[field.key], (v) => setSqlFormat(field.key, v))}
            {:else if field.kind === 'tabWidth'}
              {@render segmented(field.label, SQL_TAB_WIDTHS.map((n) => ({ value: n, label: String(n) })), sqlFmt.tabWidth, (v) => setSqlFormat('tabWidth', v))}
            {:else if field.kind === 'operatorNewline'}
              {@render segmented(field.label, [{ value: 'before', label: 'Before' }, { value: 'after', label: 'After' }], sqlFmt.logicalOperatorNewline, (v) => setSqlFormat('logicalOperatorNewline', v))}
            {:else}
              <div class="flex items-center gap-1">
                <Button type="button" variant="ghost" size="icon" class="size-7" aria-label={`Decrease ${field.label}`} onclick={() => bumpSqlFormat(field, -1)}>
                  <Minus class="size-3.5 shrink-0" />
                </Button>
                <span class="min-w-10 text-center font-mono text-ui-xs tabular-nums text-foreground">{sqlFmt[field.key]}</span>
                <Button type="button" variant="ghost" size="icon" class="size-7" aria-label={`Increase ${field.label}`} onclick={() => bumpSqlFormat(field, 1)}>
                  <Plus class="size-3.5 shrink-0" />
                </Button>
              </div>
            {/if}
          </div>
        {/if}
      {/if}
    {/each}
  {/if}
{/snippet}

<!-- Inline choice control for two-to-three short options. A popover select costs a
     floating layer, a focus trap and a search box; for "Upper / Lower / Preserve"
     the choices fit on screen, so the whole row is cheaper AND quicker to read. -->
{#snippet segmented(
  /** @type {string} */ ariaLabel,
  /** @type {Array<{ value: any, label: string }>} */ options,
  /** @type {any} */ value,
  /** @type {(v: any) => void} */ onpick,
)}
  <div role="radiogroup" aria-label={ariaLabel} class="inline-flex shrink-0 items-stretch overflow-hidden rounded-md border border-border/60">
    {#each options as o, i (o.value)}
      <button
        type="button"
        role="radio"
        aria-checked={value === o.value}
        class={cn(
          'h-7 px-2.5 text-ui-xs transition-colors',
          i > 0 && 'border-l border-border/60',
          value === o.value
            ? 'bg-muted/70 font-medium text-foreground'
            : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground',
        )}
        onclick={() => onpick(o.value)}
      >
        {o.label}
      </button>
    {/each}
  </div>
{/snippet}

{#snippet agentContent()}
  {@render secLabel('Model')}
  {#if show('Default model', 'The AI model used for chat, SQL suggestions and agent actions')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Default model</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Used for chat, SQL suggestions and agent actions.</p>
      </div>
      {#if $aiProfiles.length}
        <SelectMenu
          ariaLabel="Default model"
          placeholder="Select model"
          searchPlaceholder="Search models…"
          value={$activeProfileId}
          onValueChange={setModelProfile}
          items={$aiProfiles.map((p) => ({ value: p.id, label: p.name, keywords: [p.name, p.model] }))}
        />
      {:else}
        <span class="text-ui-xs text-muted-foreground">No models configured</span>
      {/if}
    </div>
  {/if}
  {#if show('Models & API keys', 'Add providers, models and API keys')}
    {@render actionRow('Models & API keys', 'Add providers, choose models and store API keys for OpenAI, Google Gemini, Anthropic and OpenRouter.', 'Manage', openModelConfiguration)}
  {/if}

  {@render secLabel('Chat UI')}
  {#if show('Chat font size', 'Text size for AI chat messages')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Chat font size</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Text size for AI chat messages.</p>
      </div>
      <SelectMenu
        ariaLabel="Chat font size"
        value={String(settings.agentChatFontSize)}
        onValueChange={setAgentChatFont}
        items={AGENT_FONT_SIZES.map((s) => ({ value: String(s), label: `${s}px` }))}
      />
    </div>
  {/if}
  {#if show('Code font size', 'Text size for code blocks in chat')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Code font size</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Text size for code blocks in chat.</p>
      </div>
      <SelectMenu
        ariaLabel="Code font size"
        value={String(settings.agentCodeFontSize)}
        onValueChange={setAgentCodeFont}
        items={AGENT_FONT_SIZES.map((s) => ({ value: String(s), label: `${s}px` }))}
      />
    </div>
  {/if}
  {#if show('Thinking style', 'How the thinking indicator animates while the model responds')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Thinking style</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">How the thinking indicator animates while the model responds.</p>
      </div>
      <SelectMenu
        ariaLabel="Thinking style"
        value={settings.agentThinkingStyle}
        onValueChange={setAgentThinking}
        items={THINKING_STYLES.map((o) => ({ value: o.id, label: o.label, keywords: [o.label] }))}
      />
    </div>
  {/if}
  {#if show('Web access', 'Let the agent search the web and read pages')}
    {@render switchRow(
      'Web access',
      'Let the agent search the web. Your search terms leave your machine.',
      settings.agentWebAccess,
      toggleAgentWebAccess,
    )}
  {/if}
  {#if show('Show query cards', 'Display the SQL the agent ran and the rows it returned')}
    {@render switchRow(
      'Show query cards',
      'Show the SQL the agent ran and the rows it returned. Failures always show.',
      settings.agentShowQueryCards,
      toggleAgentQueryCards,
    )}
  {/if}
{/snippet}

{#snippet generalContent()}
  {@render secLabel($t('settings.sec.startup'))}
  {#if show($t('settings.launchAtLogin'), $t('settings.launchAtLogin.desc'))}
    {@render switchRow($t('settings.launchAtLogin'), $t('settings.launchAtLogin.desc'), launchAtLogin ?? false, toggleLaunchAtLogin)}
  {/if}
  {#if show($t('settings.autoReconnect'), $t('settings.autoReconnect.desc'))}
    {@render switchRow($t('settings.autoReconnect'), $t('settings.autoReconnect.desc'), settings.autoReconnectOnStartup, toggleAutoReconnect)}
  {/if}
  {#if show('Reopen tabs', 'Restore the tabs a database had open when you connect to it again')}
    {@render switchRow(
      'Reopen tabs',
      'Connecting to a database opens the tabs it had open last time.',
      settings.reopenTabs !== false,
      () => { settings = updateSettings({ reopenTabs: settings.reopenTabs === false }); },
    )}
  {/if}
  {#if show($t('settings.previewSql'), $t('settings.previewSql.desc'))}
    {@render switchRow($t('settings.previewSql'), $t('settings.previewSql.desc'), settings.previewDmlBeforeApply, togglePreviewDml)}
  {/if}
  {#if show('Wrap JSON', 'Soft-wrap long values in every JSON viewer instead of scrolling sideways')}
    {@render switchRow('Wrap JSON', 'Soft-wrap long values (embeddings, document chunks) in every JSON view instead of running off the right edge. Off keeps the structure easier to scan.', settings.jsonWordWrap, toggleJsonWordWrap)}
  {/if}
  {#if show('Load large values on demand', 'Fetch a column that averages megabytes as a size, and load a cell when you open it')}
    {@render switchRow('Load large values on demand', 'PostgreSQL only. A column averaging half a megabyte a row shows each value\'s size, and the value loads when you open the cell. Off fetches every value with the page, which can make such a table take seconds to open.', settings.lazyWideColumns, toggleLazyWideColumns)}
  {/if}
  {#if show($t('settings.mcpAutostart'), $t('settings.mcpAutostart.desc'))}
    {@render switchRow($t('settings.mcpAutostart'), $t('settings.mcpAutostart.desc'), settings.mcpAutoStart, toggleMcpAutoStart)}
  {/if}

  {@render secLabel('Security')}
  {#if show('App PIN', 'Require a PIN to open Stroke and to connect to a database')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="flex items-center gap-2 text-ui-sm font-medium text-foreground">
          App PIN
          <span class="rounded-full border px-1.5 py-px text-ui-3xs font-medium leading-4 {$lockStatus.enabled ? 'border-primary/40 bg-primary/10 text-primary' : 'border-border/60 text-muted-foreground'}">
            {$lockStatus.enabled ? 'On' : 'Off'}
          </span>
        </p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          A {$lockStatus.pinLength}-digit PIN that locks Stroke on launch. There is no reset,
          so pick one you will remember.
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        {#if $lockStatus.enabled}
          <Button type="button" variant="ghost" size="sm" onclick={() => openPinDialog('remove')}>Remove</Button>
          <Button type="button" variant="outline" size="sm" onclick={() => openPinDialog('change')}>Change</Button>
        {:else}
          <Button type="button" variant="outline" size="sm" onclick={() => openPinDialog('set')}>Set PIN</Button>
        {/if}
      </div>
    </div>
  {/if}
  {#if $lockStatus.enabled}
    {#if show('Ask when connecting', 'Confirm the PIN before opening or reconnecting to a database')}
      {@render switchRow(
        'Ask when connecting',
        'Ask for the PIN before connecting. The reconnect right after unlocking is exempt.',
        $lockStatus.requireOnConnect,
        () => void updateLockPrefs({ requireOnConnect: !$lockStatus.requireOnConnect }),
      )}
    {/if}
    {#if show('Auto-lock', 'Lock again after a stretch of inactivity')}
      <div class={rowCls}>
        <div class="min-w-0">
          <p class="text-ui-sm font-medium text-foreground">Auto-lock</p>
          <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
            Lock again after this much inactivity. Open tabs and queries are kept.
          </p>
        </div>
        {@render segmented('Auto-lock after', AUTO_LOCK_OPTIONS, $lockStatus.autoLockMinutes, (v) => void updateLockPrefs({ autoLockMinutes: v }))}
      </div>
    {/if}
    {#if show('Lock now', 'Lock Stroke immediately')}
      {@render actionRow('Lock now', 'Lock Stroke immediately, without waiting for the auto-lock.', 'Lock', () => { open = false; lockNow(); })}
    {/if}
  {/if}

  {@render secLabel('Updates')}
  {#if show('Version', 'Current version, check for updates')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="flex items-baseline gap-2 text-ui-sm font-medium text-foreground">
          Version
          {#if appVersion}<span class="min-w-0 truncate font-mono text-ui-xs font-normal tabular-nums text-muted-foreground" title={appVersion}>{appVersion}</span>{/if}
        </p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          {#if $updateState.status === 'ready'}
            {$updateState.version} is downloaded. {$updateState.applyOnQuit ? 'It installs when you quit.' : 'Restart to install it.'}
          {:else if $updateState.status === 'downloading'}
            Downloading {$updateState.version} in the background.
          {:else if $updateState.status === 'available'}
            {$updateState.version} is available.
          {:else}
            Current version of the application.
          {/if}
        </p>
      </div>
      {#if $updateState.status === 'ready'}
        <Button type="button" size="sm" class="shrink-0 gap-1.5" onclick={() => updateActions.restart()}><Icon name="refresh-cw" class="size-3.5 shrink-0" />Restart to update</Button>
      {:else if $updateState.status === 'downloading'}
        <Button type="button" variant="outline" size="sm" class="shrink-0 gap-1.5 tabular-nums" onclick={() => updateActions.show()}><Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />{$updateState.progress}%</Button>
      {:else if $updateState.status === 'available'}
        <Button type="button" size="sm" class="shrink-0" onclick={() => updateActions.show()}>Update</Button>
      {:else if $updateState.checking}
        <Button type="button" variant="outline" size="sm" class="shrink-0 gap-1.5" disabled><Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />Checking</Button>
      {:else}
        <Button type="button" variant="outline" size="sm" class="shrink-0" onclick={checkUpdates}>{upToDate ? 'Up to date' : 'Check for updates'}</Button>
      {/if}
    </div>
  {/if}
  {#if show('Update track', 'Stable or Nightly')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Update track</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Nightly is built every night from the newest code. Each track is its own app with its own settings, so switching leaves the other as it is.
        </p>
      </div>
      <div class="shrink-0">
        <SelectMenu
          triggerClass="h-8 w-32"
          contentClass="w-[var(--bits-popover-anchor-width)] min-w-[8rem]"
          ariaLabel="Update track"
          value={onNightly ? 'nightly' : 'stable'}
          onValueChange={(v) => void askSwitch(v)}
          items={[{ value: 'stable', label: 'Stable', keywords: ['stable'] }, { value: 'nightly', label: 'Nightly', keywords: ['nightly', 'beta'] }]}
        />
      </div>
    </div>
  {/if}
  {#if show('Download updates in the background', 'Install new versions automatically when you quit')}
    {@render switchRow(
      'Download updates in the background',
      'New versions download while you work and install when you quit Stroke. Off asks before each download.',
      settings.autoUpdate,
      toggleAutoUpdate,
    )}
  {/if}

  <!-- Privacy lives in General, not Agent: this switch covers the whole app, and
       under Agent it read as if it were about the AI features alone. -->
  {@render secLabel('Privacy')}
  {#if show('Anonymous usage data', 'Help decide what to build next')}
    {@render switchRow(
      'Anonymous usage data',
      'Which features you use, the app version and your OS. Never queries, names or data.',
      settings.telemetry,
      toggleTelemetry,
    )}
  {/if}
{/snippet}

{#snippet appearanceContent()}
  {@render secLabel($t('settings.sec.themeTypeface'))}

  {#if show($t('settings.theme'), `${$t('settings.theme.desc')} dark mode light mode appearance colour color`)}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.theme')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.theme.desc')}</p>
      </div>
      <SearchableMenu
        align="end"
        contentClass="min-w-72"
        placeholder="Search themes…"
        items={themeGroups.flatMap((g) => g.themes.map((t) => ({ value: t.id, label: t.name, keywords: [g.label], bg: t.preview.bg, accent: t.preview.accent })))}
        onselect={(it) => setTheme(/** @type {import('$lib/themes/registry.js').ThemeId} */ (it.value))}
      >
        {#snippet trigger(props)}
          <button
            {...props}
            type="button"
            aria-label="Color theme"
            class={cn(
              "flex h-8 w-56 items-center justify-between gap-2 whitespace-nowrap rounded-[10px] border-2 border-border/70 bg-background px-2.5 text-ui-xs font-normal shadow-none outline-none transition-colors hover:bg-muted/30 focus-visible:border-ring/55 focus-visible:ring-1 focus-visible:ring-ring/18 data-[state=open]:border-ring",
            )}
          >
            <span class="flex min-w-0 items-center gap-2">
              <ThemeSwatch bg={activeTheme.preview.bg} accent={activeTheme.preview.accent} />
              <span class="truncate font-medium">{activeTheme.name}</span>
            </span>
            <Icon name="chevron-down" class="size-3.5 shrink-0 opacity-40" />
          </button>
        {/snippet}
        {#snippet item(it)}
          <span class="min-w-0 flex-1 truncate text-ui-xs font-medium">{it.label}</span>
          {#if $appThemeId === it.value}<Icon name="check" class="ml-auto size-3.5 shrink-0 text-primary" />{/if}
          <ThemeSwatchStrip themeId={/** @type {import('$lib/themes/registry.js').ThemeId} */ (it.value)} />
        {/snippet}
      </SearchableMenu>
    </div>
  {/if}

  {#if show('Theme builder', 'Custom theme colours intensity grain noise')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Theme builder</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Make your own theme from colours, intensity and film grain. Previews live.</p>
      </div>
      <Button type="button" variant="outline" size="sm" onclick={() => (themeBuilderOpen = true)}>
        <Icon name="sparkles" class="size-3.5 shrink-0" />
        {$appThemeId === 'custom' ? 'Edit theme' : 'Build a theme'}
      </Button>
    </div>
  {/if}

  {#if show('Add a theme', 'Import theme community themes vs code json file paste gallery')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Add a theme</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">From stroke.click/themes, a Stroke or VS Code theme file, or pasted JSON.</p>
      </div>
      <Button type="button" variant="outline" size="sm" onclick={() => (addThemeOpen = true)}>
        <Icon name="plus" class="size-3.5 shrink-0" />
        Add a theme
      </Button>
    </div>
    {#if $importedThemes.length}
      <ul class="-mt-1 flex flex-col pb-2">
        {#each $importedThemes as it (it.id)}
          {@const on = $appThemeId === 'imported' && $activeImportedId === it.id}
          <li class="group flex h-9 items-center gap-2.5 rounded-md px-2 hover:bg-muted/40">
            <ThemeSwatch bg={it.colors.background} accent={it.colors.primary} />
            <button type="button" class="flex min-w-0 flex-1 items-center gap-2 text-left" onclick={() => useImported(it.id)}>
              <span class="truncate text-ui-xs font-medium text-foreground">{it.name}</span>
              <span class="truncate text-ui-2xs text-muted-foreground">{it.appearance === 'light' ? 'Light' : 'Dark'}{it.author ? ` · ${it.author}` : ''}</span>
            </button>
            {#if on}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
            <button type="button" aria-label={`Remove ${it.name}`} class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 hover:bg-muted hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100" onclick={() => dropImported(it.id)}>
              <Icon name="trash-2" class="size-3.5 shrink-0" />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if show($t('settings.font'), $t('settings.font.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.font')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.font.desc')}</p>
      </div>
      <SelectMenu
        ariaLabel="Font family"
        value={settings.font}
        onValueChange={(v) => { if (v) setFont(/** @type {import('$lib/stores/settings.js').FontId} */ (v)); }}
        items={fontEntries.map(([id, preset]) => ({ value: id, label: preset.label, sans: preset.sans, keywords: [preset.label, preset.description, id === 'mono' ? 'monospace' : 'sans', ...(preset.keywords ?? [])] }))}
        searchPlaceholder="Search fonts…"
      >
        {#snippet lead(it)}
          <span class="flex size-5 shrink-0 items-center justify-center rounded border border-border/40 bg-muted/30 text-ui-2xs font-semibold text-foreground/70" style="font-family: {it.sans}" aria-hidden="true">Aa</span>
        {/snippet}
      </SelectMenu>
    </div>
  {/if}

  {#if show($t('settings.language'), $t('settings.language.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.language')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.language.desc')}.</p>
      </div>
      <SelectMenu
        ariaLabel="Language"
        value={$locale}
        onValueChange={(v) => { if (v) setLocale(/** @type {any} */ (v)); }}
        items={LOCALES.map((l) => ({ value: l.id, label: l.native, hint: l.label, keywords: [l.native, l.label] }))}
        searchPlaceholder="Search languages…"
      />
    </div>
  {/if}

  {@render secLabel($t('settings.sec.icons'))}

  {#if show($t('settings.iconWeight'), $t('settings.iconWeight.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.iconWeight')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.iconWeight.desc')}</p>
      </div>
      <SelectMenu
        ariaLabel="Icon style"
        value={settings.iconStyle}
        onValueChange={(v) => { if (v) setIconStyle(/** @type {import('$lib/stores/settings.js').IconStyleId} */ (v)); }}
        items={iconStyleEntries.map(([id, preset]) => ({ value: id, label: preset.label, strokeWidth: preset.strokeWidth }))}
      >
        {#snippet lead(it)}
          <PenTool class="size-4 shrink-0 text-muted-foreground" style="stroke-width: {it.strokeWidth}px" aria-hidden="true" />
        {/snippet}
      </SelectMenu>
    </div>
  {/if}

  {#if show($t('settings.iconSet'), $t('settings.iconSet.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.iconSet')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.iconSet.desc')}</p>
      </div>
      <SelectMenu
        ariaLabel="Icon set"
        value={settings.iconSet}
        onValueChange={(v) => { if (v) setIconSet(/** @type {import('$lib/stores/settings.js').IconSetId} */ (v)); }}
        items={iconSetEntries.map(([id, preset]) => ({ value: id, label: preset.label, keywords: [preset.label] }))}
      >
        {#snippet lead(it)}
          <span class="flex size-4 shrink-0 items-center justify-center text-muted-foreground">
            {#if it.value === "hugeicons"}
              <HugeiconsIcon icon={SparklesIcon} class="size-4" strokeWidth={1.8} />
            {:else if it.value === "phosphor"}
              <PhosphorSparkle class="size-4" size="100%" />
            {:else}
              <LucideSparkles class="size-4 shrink-0" />
            {/if}
          </span>
        {/snippet}
      </SelectMenu>
    </div>
  {/if}

  {#if show($t('settings.tableStyle'), $t('settings.tableStyle.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.tableStyle')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.tableStyle.desc')}</p>
      </div>
      <SelectMenu
        ariaLabel="Table style"
        value={settings.tableStyle}
        onValueChange={(v) => { if (v) setTableStyle(/** @type {import('$lib/stores/settings.js').TableStyleId} */ (v)); }}
        items={tableStyleEntries.map(([id, preset]) => ({ value: id, label: preset.label, keywords: [preset.label] }))}
      >
        {#snippet lead(it)}
          <span class="size-4 shrink-0 overflow-hidden rounded-[3px] border border-border/40 bg-background" style={tableStylePreview[it.value] ?? ''} aria-hidden="true"></span>
        {/snippet}
      </SelectMenu>
    </div>
  {/if}
  {#if show('JSON colours', 'Palette for JSON keys, strings, numbers and booleans')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">JSON colours</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Colours for JSON keys, strings, numbers and booleans. Auto follows the app theme.
        </p>
      </div>
      <SelectMenu
        ariaLabel="JSON colours"
        value={settings.jsonTheme}
        onValueChange={setJsonTheme}
        items={jsonThemeEntries.map(([id, p]) => ({ value: id, label: p.label, keywords: [p.label, p.description] }))}
      >
        {#snippet lead(it)}
          <!-- The palette itself, which is the only description that matters. -->
          <span class="flex shrink-0 items-center gap-px" data-json-theme={it.value} aria-hidden="true">
            <span class="size-1.5 rounded-[1px]" style="background:var(--json-key)"></span>
            <span class="size-1.5 rounded-[1px]" style="background:var(--json-string)"></span>
            <span class="size-1.5 rounded-[1px]" style="background:var(--json-number)"></span>
            <span class="size-1.5 rounded-[1px]" style="background:var(--json-boolean)"></span>
          </span>
        {/snippet}
      </SelectMenu>
    </div>
  {/if}
  {#if show('Editor theme', 'Colours for the SQL editor and every code view: One Dark, GitHub, Dracula, Monokai, Nord, Solarized, Tokyo Night, Catppuccin, Rose Pine, Gruvbox')}
    <!-- Not a rowCls row: the gallery needs the full width under its label. -->
    <div class="border-t border-border/25 py-3.5">
      <p class="text-ui-sm font-medium text-foreground">Editor theme</p>
      <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
        Colours for the SQL editor and every other code view. Match app theme uses the app theme and the JSON colours above.
      </p>
      <div class="mt-3">
        <EditorThemePicker value={settings.editorTheme ?? 'app'} onpick={setEditorTheme} />
      </div>
    </div>
  {/if}
  {#if show('Row spacing', 'Vertical space each row of the data grid takes')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Row spacing</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Row height in the data grid. Compact fits about a third more rows on screen.
        </p>
      </div>
      <SelectMenu
        ariaLabel="Row spacing"
        value={settings.rowSpacing}
        onValueChange={setRowSpacing}
        items={rowSpacingEntries.map(([id, s]) => ({ value: id, label: s.label, keywords: [s.label] }))}
      />
    </div>
  {/if}
  {#if show('Notifications', 'Corner of the window where notifications appear: top left, top right, bottom left, bottom right')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Notifications</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          The corner of the window they appear in.
        </p>
      </div>
      <!-- A small window with four corners to pick: the choice is spatial, so
           it is shown as a place rather than named in a list. -->
      <div class="grid h-12 w-[4.5rem] shrink-0 grid-cols-2 grid-rows-2 gap-1 rounded-md border border-border/60 bg-muted/30 p-1" role="radiogroup" aria-label="Notification position">
        {#each TOAST_POSITIONS as pos (pos)}
          <button
            type="button"
            role="radio"
            aria-checked={$toastPosition === pos}
            aria-label={pos.replace('-', ' ')}
            title={pos.replace('-', ' ').replace(/^./, (c) => c.toUpperCase())}
            class={cn(
              'flex rounded-sm transition-[background-color,scale] duration-[120ms] ease-[var(--ease-out)] hover:bg-accent active:scale-[0.94] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring',
              pos.startsWith('top') ? 'items-start' : 'items-end',
              pos.endsWith('left') ? 'justify-start' : 'justify-end',
            )}
            onclick={() => {
              toastPosition.set(pos)
              toast.success('Notifications appear here now')
            }}
          >
            <span class={cn('m-0.5 h-1.5 w-4 rounded-sm', $toastPosition === pos ? 'bg-primary' : 'bg-muted-foreground/30')}></span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
  {#if show('Grid text size', 'Font size of the data grid, independent of the app zoom')}
    <div class="flex items-center justify-between gap-4 py-2">
      <div class="min-w-0 flex-1">
        <p class="text-ui-sm font-medium text-foreground">Grid text size</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Text size in the data grid at 100% zoom, independent of the app zoom.
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <Button variant="outline" size="icon" class="size-7" aria-label="Smaller grid text"
          disabled={settings.gridFontSize <= GRID_FONT_MIN}
          onclick={() => setGridFontSize(settings.gridFontSize - 1)}>−</Button>
        <!-- The reading doubles as the reset. A separate "Reset" control for one
             number is more chrome than the number itself; clicking the value you
             are trying to change back is where the pointer already is. Title and
             aria-label carry what it does, since the glyph cannot. -->
        <button
          type="button"
          class="hit-area w-12 rounded text-center font-mono text-ui-xs tabular-nums text-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-100 disabled:hover:bg-transparent"
          disabled={settings.gridFontSize === DEFAULT_GRID_FONT_SIZE}
          title={settings.gridFontSize === DEFAULT_GRID_FONT_SIZE ? 'Default size' : `Reset to ${DEFAULT_GRID_FONT_SIZE}px`}
          aria-label={settings.gridFontSize === DEFAULT_GRID_FONT_SIZE ? 'Grid text size, default' : `Reset grid text size to ${DEFAULT_GRID_FONT_SIZE} pixels`}
          onclick={() => setGridFontSize(DEFAULT_GRID_FONT_SIZE)}
        >{settings.gridFontSize}px</button>
        <Button variant="outline" size="icon" class="size-7" aria-label="Larger grid text"
          disabled={settings.gridFontSize >= GRID_FONT_MAX}
          onclick={() => setGridFontSize(settings.gridFontSize + 1)}>+</Button>
      </div>
    </div>
  {/if}
  {#if show('Boolean glyphs', 'Show a coloured dot or check for boolean columns')}
    {@render switchRow(
      'Boolean glyphs',
      'Draw booleans as a dot or ✓ / ✗ instead of true / false text.',
      boolGlyphOn,
      () => setPluginEnabled(BOOL_GLYPH_ID, !boolGlyphOn),
    )}
  {/if}
  {#if show('Group large numbers', 'Thousands separators on integers in the grid')}
    {@render switchRow(
      'Group large numbers',
      'Thousands separators on integers: 162957 reads as 162,957. Decimals are untouched.',
      settings.numberGrouping,
      toggleNumberGrouping,
    )}
  {/if}
  {#if show('Image previews', 'Show thumbnails for image URLs in the grid')}
    {@render switchRow(
      'Image previews',
      'Thumbnail cells holding an image URL. Off stops the download, not just the drawing.',
      settings.imagePreview,
      toggleImagePreview,
    )}
  {/if}
  {#if show('Open links on click', 'Clicking a URL cell opens it in your browser')}
    {@render switchRow(
      'Open links on click',
      'Click a URL cell to open it in your browser. Off, a click only selects the cell.',
      settings.openUrlsOnClick,
      toggleOpenUrls,
    )}
  {/if}
  {#if show('Highlight the active row', 'Tint the row the keyboard is on')}
    {@render switchRow(
      'Highlight the active row',
      'Tint the full row holding the focused cell. The cell keeps its outline either way.',
      settings.highlightActiveRow,
      toggleHighlightActiveRow,
    )}
  {/if}
  {#if show('Expand single related row', 'Show a lone related row as JSON in the dock')}
    {@render switchRow(
      'Expand single related row',
      'When following a foreign key opens a sub view with exactly one row, that row opens as JSON.',
      settings.fkAutoExpandJson,
      toggleFkAutoExpandJson,
    )}
  {/if}
  {#if show('Rows per page', 'How many rows a newly opened table fetches, page size, limit')}
    <div class="flex items-center justify-between gap-4 py-2">
      <div class="min-w-0 flex-1">
        <p class="text-ui-sm font-medium text-foreground">Rows per page</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          How many rows a newly opened table fetches. Same value as the toolbar control.
        </p>
      </div>
      <SelectMenu
        ariaLabel="Rows per page"
        value={String(defaultPageSize)}
        onValueChange={setDefaultPageSize}
        items={pageSizeItems}
      />
    </div>
  {/if}
  {#if show('Empty and NULL markers', 'Tell NULL, empty string and whitespace-only cells apart')}
    {@render switchRow(
      'Empty and NULL markers',
      'Draw NULL as ∅, an empty string as "" and whitespace as ·····, so blanks differ.',
      nullishOn,
      () => setPluginEnabled(NULLISH_ID, !nullishOn),
    )}
  {/if}
  {#if show('Sidebar position', 'Which side of the window the sidebar sits on')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Sidebar position</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Which side of the window the sidebar sits on.
        </p>
      </div>
      <SelectMenu
        ariaLabel="Sidebar position"
        value={$sidebarSideStore}
        onValueChange={(v) => { if (v === 'left' || v === 'right') setSidebarSide(v) }}
        items={sidebarSideItems.map((i) => ({ ...i, keywords: [i.label] }))}
      />
    </div>
  {/if}
  {#if show('Comments in the sidebar', 'Table, view and routine comments as a second line under their names')}
    {@render switchRow(
      'Comments in the sidebar',
      'Show the comment a table, view or routine carries in the catalog as a second line under its name. Read once per schema.',
      settings.sidebarComments,
      () => (settings = updateSettings({ sidebarComments: !settings.sidebarComments })),
    )}
  {/if}
  {#if show('Remember open groups', 'The Objects tab reopens the groups you had open, per connection')}
    {@render switchRow(
      'Remember open groups',
      'The Objects tab reopens the groups you left open, per connection. Off, Views and Functions start open and the rest folded.',
      settings.sidebarRememberGroups,
      () => (settings = updateSettings({ sidebarRememberGroups: !settings.sidebarRememberGroups })),
    )}
  {/if}
  {#if show('Motion', 'How much the interface animates')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Motion</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          System follows your OS reduced-motion setting. Spinners keep turning either way.
        </p>
      </div>
      <SelectMenu
        ariaLabel="Motion"
        value={settings.motion}
        onValueChange={setMotion}
        items={motionEntries.map(([id, m]) => ({ value: id, label: m.label, keywords: [m.label, m.description] }))}
      />
    </div>
  {/if}
  {#if show('Animate panels', 'Slide the terminal and the AI panel open and closed')}
    {@render switchRow(
      'Animate panels',
      'The terminal dock and the AI panel slide open and closed. Off, they appear at once.',
      settings.animatePanels === true,
      () => { settings = updateSettings({ animatePanels: settings.animatePanels !== true }); },
    )}
  {/if}
  {#if isMacOs && show('Press ⌘Q twice to quit', 'Quit confirm command q hold double press')}
    {@render switchRow(
      'Press ⌘Q twice to quit',
      'The first ⌘Q shows a reminder; press it again or hold it to quit. Off, ⌘Q quits at once.',
      settings.quitGuard !== false,
      () => { settings = updateSettings({ quitGuard: settings.quitGuard === false }); },
    )}
  {/if}
  {#if show('Alternating row colors', 'Shade every other grid row')}
    {@render switchRow(
      'Alternating row colors',
      'Shade every other row. The Striped and Dots grid styles already do this.',
      settings.zebraRows,
      toggleZebraRows,
    )}
  {/if}
  {#if show('Menu bar', 'File, Edit, View, Tools and Help in the title bar')}
    {@render switchRow(
      'Menu bar',
      `Show File, Edit, View, Tools and Help in the title bar. All of it is also in ${comboTitle('Mod+K')}.`,
      settings.showMenuBar,
      toggleMenuBar,
    )}
  {/if}
  {#if show('Row numbers', 'Number every grid row in the gutter')}
    {@render switchRow(
      'Row numbers',
      'Number rows in the gutter, counting from the first row of the page.',
      settings.showRowNumbers,
      toggleRowNumbers,
    )}
  {/if}
  {#if show('Referenced-by columns', 'Columns linking to rows in tables whose foreign keys point here')}
    {@render switchRow(
      'Referenced-by columns',
      'A column for each table whose foreign key points at this one, linking to the rows that reference the row. Off, they are not loaded or drawn.',
      settings.showRelationColumns !== false,
      toggleRelationColumns,
    )}
  {/if}
  {#if show('Cell alignment', 'Which side grid cell text sits on')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Cell alignment</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">
          Which side cell text sits on. Numbers right lines digits up by place value.
        </p>
      </div>
      <SelectMenu
        ariaLabel="Cell alignment"
        value={settings.tableTextAlign}
        onValueChange={setTableAlign}
        items={TABLE_ALIGN_OPTIONS.map((o) => ({ value: o.id, label: o.label, keywords: [o.label] }))}
      />
    </div>
  {/if}

  {#if show('Native scrolling', 'Let the OS scroll the grid and the sidebar instead of the app\'s eased scrolling')}
    {@render switchRow(
      'Native scrolling',
      "Hand the data grid and the sidebar back to the OS scroller. Off (the default) the app eases each wheel tick to its destination, which reads as smoother on mice that scroll in big jumps; on, scrolling behaves exactly like the rest of your desktop (including its own momentum) and nothing of ours sits in front of the wheel.",
      settings.nativeScroll,
      toggleNativeScroll,
    )}
  {/if}

  {@render secLabel($t('settings.sec.display'))}
  {#if show($t('settings.zoom'), $t('settings.zoom.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.zoom')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.zoom.desc')}</p>
      </div>
      <div class="flex items-center gap-1">
        <Button type="button" variant="ghost" size="icon" class="size-7" aria-label="Zoom out" disabled={!canDecreaseZoom(settings.zoom)} onclick={() => bumpZoom(-1)}>
          <Minus class="size-3.5 shrink-0" />
        </Button>
        <button type="button" class="min-w-12 rounded-md px-2 py-1 font-mono text-ui-xs tabular-nums text-foreground transition-[background-color,transform] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] hover:bg-muted active:scale-[0.98]" onclick={() => (settings = resetZoom())} title="Reset to 100%">{zoomLabel}</button>
        <Button type="button" variant="ghost" size="icon" class="size-7" aria-label="Zoom in" disabled={!canIncreaseZoom(settings.zoom)} onclick={() => bumpZoom(1)}>
          <Plus class="size-3.5 shrink-0" />
        </Button>
      </div>
    </div>
  {/if}
{/snippet}

{#snippet integrationsContent()}
  {@render secLabel($t('settings.sec.tools'))}
  {#if show($t('settings.extensions'), $t('settings.extensions.desc'))}
    {@render actionRow($t('settings.extensions'), $t('settings.extensions.desc'), $t('settings.btn.open'), () => { open = false; onopenextensions(); })}
  {/if}
  {#if show($t('settings.mcpConfig'), $t('settings.mcpConfig.desc'))}
    {@render actionRow($t('settings.mcpConfig'), $t('settings.mcpConfig.desc'), $t('settings.btn.open'), () => { open = false; onopenmcp(); })}
  {/if}
  {#if show($t('settings.aiModels'), $t('settings.aiModels.desc'))}
    {@render actionRow($t('settings.aiModels'), $t('settings.aiModels.desc'), $t('settings.btn.configure'), openModelConfiguration)}
  {/if}
{/snippet}

<!-- Account & Sync: who you are, your plan and devices, and end-to-end
     encrypted sync, in one place instead of under Integrations and the
     License page. -->
{#snippet accountContent()}
  {@render secLabel($t('settings.sec.account'))}
  {#if show('Account', 'Sign in with GitHub or Google, plan, devices, sign out')}
    <AccountCard variant="row" />
  {/if}
  {#if show($t('settings.license'), $t('settings.license.desc'))}
    {@render actionRow($t('settings.license'), $t('settings.license.desc'), $t('settings.btn.manage'), () => { open = false; onopenlicense(); }, planBadge)}
  {/if}
  {#if show('Sync', 'End-to-end encrypted sync of connections, workspaces and settings across your devices')}
    {#if $isSignedIn}
      <div class="mt-6">
        <SyncSection sectionLabel="mb-1 text-ui-2xs font-semibold uppercase tracking-[0.06em] text-muted-foreground" />
      </div>
    {:else}
      {@render secLabel('Sync')}
      <div class={rowCls}>
        <div class="min-w-0">
          <p class="text-ui-sm font-medium text-foreground">Sync your connections</p>
          <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Sign in to keep connections, workspaces and settings on all your devices. They are encrypted on this device with a passphrase only you know, so no one else can read them, Stroke included.</p>
        </div>
        <Button type="button" size="sm" class="shrink-0" onclick={() => void signInWithToast()}>Sign in</Button>
      </div>
    {/if}
  {/if}

  {@render secLabel('Sharing')}
  {#if show('Shared links', 'Share links: every link you made, its views and expiry, revoke, my shares')}
    {@render actionRow('Shared links', 'Every link you\'ve shared, how often it was opened, when it expires. Revoke any of them at once.', 'Open', () => { open = false; openSharesPage(); })}
  {/if}
  {#if show('Default expiry', 'Share links: how long a new link lasts')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Default expiry</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">How long a new share link lasts. You can change it for each link.</p>
      </div>
      {@render segmented('Default expiry', SHARE_EXPIRIES.filter((e) => e.seconds !== null).map((e) => ({ value: e.seconds, label: e.label })), shareDefaults.expiry, (v) => setShareDefault({ expiry: v }))}
    </div>
  {/if}
  {#if show('Default views', 'Share links: anyone, one view then gone, burn after reading, limited views')}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">Default views</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">Whether a new link opens any number of times, once, or a set number of times.</p>
      </div>
      {@render segmented('Default views', [{ value: 'link', label: 'Unlimited' }, { value: 'once', label: 'Once' }, { value: 'limit', label: `${shareDefaults.viewLimit} views` }], shareDefaults.access, (v) => setShareDefault({ access: v }))}
    </div>
  {/if}
  {#if show('Mask personal data', 'Share links: hide emails, phone numbers, names, tokens, PII')}
    {@render switchRow('Mask personal data', 'Columns that look like emails, phone numbers, names or secrets start masked in the share dialog.', shareDefaults.maskPii, () => setShareDefault({ maskPii: !shareDefaults.maskPii }))}
  {/if}
{/snippet}

{#snippet aboutContent()}
  {@render secLabel($t('settings.sec.about'))}
  {#if show($t('settings.aboutStroke'), $t('settings.aboutStroke.desc'))}
    {@render actionRow($t('settings.aboutStroke'), $t('settings.aboutStroke.desc'), $t('settings.btn.view'), () => { open = false; onopenabout(); })}
  {/if}
  {#if show($t('settings.website'), $t('settings.website.desc'))}
    <div class={rowCls}>
      <div class="min-w-0">
        <p class="text-ui-sm font-medium text-foreground">{$t('settings.website')}</p>
        <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{$t('settings.website.desc')}</p>
      </div>
        <a href="https://stroke.click" target="_blank" rel="noopener noreferrer" class="field-surface inline-flex h-8 shrink-0 items-center gap-1.5 bg-background px-3 text-ui-xs font-medium text-foreground transition-[background-color,transform] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] hover:bg-muted active:scale-[0.98]">
        stroke.click <Icon name="external-link" class="size-3.5 shrink-0" />
      </a>
    </div>
  {/if}

  {#if !searching}
    <p class="mt-8 mb-3 text-ui-2xs font-semibold uppercase tracking-[0.06em] text-muted-foreground">{$t('settings.sec.keyboard')}</p>
    <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
      {#if bindingFor('theme.next')}{@render shortcut(bindingFor('theme.next'), $t('settings.kbd.cycleTheme'))}{/if}
      {#if bindingFor('theme.prev')}{@render shortcut(bindingFor('theme.prev'), $t('settings.kbd.prevTheme'))}{/if}
      {@render shortcut('Mod+Plus', $t('settings.kbd.zoom'))}
      {@render shortcut('Mod+Minus', $t('settings.kbd.zoom'))}
      {@render shortcut('Mod+0', $t('settings.kbd.resetZoom'))}
    </div>
  {/if}
{/snippet}

<!-- One combo in `createHotkey` grammar, printed as keycaps by <Kbd> - the same
     caps the menus and the shortcuts dialog draw. It used to take a glyph string
     and set it in a single bordered box, so `⌘⇧M` was one cap reading "⌘⇧M" and
     the ⌘ was hardcoded for every platform. -->
{#snippet shortcut(/** @type {string} */ combo, /** @type {string} */ action)}
  <span class="flex items-center gap-1.5 text-ui-2xs text-muted-foreground">
    <Kbd {combo} />
    {action}
  </span>
{/snippet}

<style>
  /* Section/search switch: a short fade. The 4px lift it had meant moving
     the whole pane, which the software renderer on Linux repaints each frame,
     and on open it ran on top of the dialog's own entrance. */
  .settings-pane {
    animation: settings-pane-in 120ms ease-out both;
  }

  @keyframes settings-pane-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .settings-pane {
      animation: none;
    }
  }
</style>

<ThemeBuilder bind:open={themeBuilderOpen} />
<AddThemeDialog bind:open={addThemeOpen} />

<!-- Switching track: what opens, what quits, and what stays. -->
<Dialog.Root open={switchTo !== null} onOpenChange={(o) => { if (!o && !switchBusy) switchTo = null }}>
  <Dialog.Content class="max-w-[30rem] gap-0 p-0" showCloseButton={false}>
    <Dialog.Header class="gap-1 px-6 pt-6">
      <Dialog.Title class="text-ui-xl font-semibold tracking-tight">Switch to {switchTo === 'nightly' ? 'Nightly' : 'Stable'}?</Dialog.Title>
      <Dialog.Description class="text-ui-sm text-muted-foreground">
        {switchName} opens and this app quits.
      </Dialog.Description>
    </Dialog.Header>
    <div class="px-6 pt-5">
      <table class="w-full text-ui-sm">
        <thead>
          <tr class="border-b border-border/60 text-start text-ui-xs text-muted-foreground">
            <th class="pb-2 pe-6 text-start font-normal">App</th>
            <th class="pb-2 text-start font-normal">Version</th>
            <th class="pb-2 text-end font-normal"></th>
          </tr>
        </thead>
        <tbody>
          <tr class="border-b border-border/40">
            <td class="py-3 pe-6 whitespace-nowrap text-foreground">{switchName}</td>
            <td class="py-3 text-muted-foreground">{switchInstalled ? 'Installed' : switchTo === 'nightly' ? 'Latest Nightly' : 'Latest release'}</td>
            <td class="py-3 text-end text-ui-xs {switchInstalled ? 'text-muted-foreground' : 'text-warning'}">{switchInstalled ? 'opens' : 'downloads, then opens'}</td>
          </tr>
          <tr>
            <td class="py-3 pe-6 whitespace-nowrap text-foreground">This app</td>
            <td class="py-3 pe-4 font-mono text-ui-xs tabular-nums text-muted-foreground">{appVersion.replace(/-nightly\.\d+$/, '')}{#if /-nightly\./.test(appVersion)}<span class="ms-1.5 font-sans text-ui-2xs">Nightly</span>{/if}</td>
            <td class="py-3 text-end text-ui-xs text-muted-foreground">quits, stays installed</td>
          </tr>
        </tbody>
      </table>
      {#if switchError}<p class="mt-3 text-ui-xs text-destructive">{switchError}</p>{/if}
    </div>
    <div class="mt-4 flex items-center gap-4 border-t border-border/40 px-6 py-4">
      <p class="min-w-0 flex-1 text-ui-xs leading-relaxed text-muted-foreground">
        Connections and settings stay with each app. Signed in, synced ones follow you.
      </p>
      <div class="flex shrink-0 items-center gap-2">
        <Button type="button" variant="outline" disabled={switchBusy} onclick={() => (switchTo = null)}>Cancel</Button>
        <Button type="button" class="gap-1.5" disabled={switchBusy} onclick={() => void confirmSwitch()}>
          {#if switchBusy}<Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" />{switchInstalled ? 'Opening' : 'Downloading'}{:else}Switch to {switchTo === 'nightly' ? 'Nightly' : 'Stable'}{/if}
        </Button>
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
