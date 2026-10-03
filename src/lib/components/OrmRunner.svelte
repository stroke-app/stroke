<script>
  import { onMount, untrack } from "svelte";
  import * as monaco from '$lib/monaco.js';
  import { configureMonacoWorkers, editorFontFamily } from "$lib/monaco-env.js";
  import {
    defineStrokeMonacoThemes,
    monacoThemeId,
    readEditorFontOptions,
  } from "$lib/monaco-themes.js";
  import { normalizeThemeId } from "$lib/themes/registry.js";
  import { appVimMode } from "$lib/stores/settings.js";
  import { setVimSubMode } from "$lib/vim/vim.js";
  import Play from "@lucide/svelte/icons/play";
  import Code2 from "@lucide/svelte/icons/code-2";
  import Copy from "@lucide/svelte/icons/copy";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import CheckCheck from "@lucide/svelte/icons/check-check";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import Braces from "@lucide/svelte/icons/braces";
  import { Button } from "$lib/components/ui/button/index.js";
  import DataTable from "./DataTable.svelte";
  import TableLoading from "./TableLoading.svelte";
  import ShikiBlock from "./ShikiBlock.svelte";
  import JsonViewer from "./JsonViewer.svelte";
  import ResizeHandle from "./ResizeHandle.svelte";
  import { cn } from "$lib/utils.js";
  import { rowsToObjects } from "$lib/export.js";
  import { evalDrizzleQuery, evalPrismaQuery } from "$lib/orm-builder.js";
  import {
    clampSqlEditorHeight,
    loadLayout,
    saveLayout,
  } from "$lib/stores/layout.js";

  /** @typedef {import('$lib/sql-complete-data.js').SqlSchemaHints} SqlSchemaHints */

  let {
    code = $bindable(""),
    mode = $bindable(/** @type {'drizzle' | 'prisma'} */ ("drizzle")),
    columns = [],
    rows = [],
    loading = false,
    error = "",
    queryMs = 0,
    schemaHints = /** @type {SqlSchemaHints} */ ({}),
    onrun = /** @type {(detail: { sql: string, mode: string }) => void} */ (
      () => {}
    ),
    onmodi = undefined,
    onmodw = undefined,
    onmodn = undefined,
    onmodm = undefined,
    onmodt = undefined,
    onmodshifte = undefined,
    onmodshiftd = undefined,
    onmodaltd = undefined,
  } = $props();

  /** @type {HTMLElement | null} */
  let container = $state(null);
  /** @type {HTMLElement | null} */
  let consoleEl = $state(null);
  /** @type {monaco.editor.IStandaloneCodeEditor | null} */
  let editor = null;
  /** Non-reactive editor come-alive flag for the Vim effect. */
  let editorReady = $state(false);
  /** Host element for the monaco-vim mode status strip. */
  let vimStatusEl = $state(/** @type {HTMLElement | null} */ (null));

  const initialLayout = loadLayout();
  let editorHeight = $state(initialLayout.sqlEditorHeight);
  let resizeStartHeight = initialLayout.sqlEditorHeight;

  let generatedSql = $state("");
  let sqlPreviewOpen = $state(true);
  let parseError = $state("");

  /** @type {'table' | 'json'} */
  let outputView = $state("table");

  let copied = $state(
    /** @type {'sql' | 'code' | 'json' | 'error' | 'parse' | null} */ (null),
  );
  /** @type {ReturnType<typeof setTimeout> | null} */
  let copiedTimer = null;

  /** Per-mode code storage so switching modes doesn't wipe user code. */
  /** @type {{ drizzle: string; prisma: string }} */
  const codeByMode = { drizzle: "", prisma: "" };

  /** @param {string} text @param {'sql' | 'code' | 'json' | 'error' | 'parse'} kind */
  function copyWithFeedback(text, kind) {
    if (!text.trim()) return;
    navigator.clipboard.writeText(text).then(() => {
      copied = kind;
      if (copiedTimer) clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => {
        copied = null;
      }, 2000);
    });
  }

  /** @param {number} height */
  function clampEditorHeight(height) {
    return clampSqlEditorHeight(height, consoleEl?.clientHeight ?? 0);
  }

  function currentTheme() {
    return normalizeThemeId(document.documentElement.dataset.theme);
  }

  /** @param {'drizzle' | 'prisma'} newMode */
  function switchMode(newMode) {
    if (newMode === mode) return;
    codeByMode[mode] = code;
    mode = newMode;
    code = codeByMode[newMode];
    // If code is empty, the defaultCode $effect will fill it in
  }

  /** @param {monaco.editor.IStandaloneCodeEditor} ed */
  function registerShortcuts(ed) {
    const { CtrlCmd, Shift, Alt } = monaco.KeyMod
    const { Enter, KeyS, KeyI, KeyW, KeyN, KeyM, KeyT, KeyD, KeyE } = monaco.KeyCode
    const run = (/** @type {(() => void) | undefined} */ fn) => fn?.()

    // Editor-local
    ed.addCommand(CtrlCmd | Enter, () => void handleRun())
    ed.addCommand(CtrlCmd | KeyS,  () => { ed.getAction("editor.action.formatDocument")?.run() })

    // Global app shortcuts
    ed.addCommand(CtrlCmd | KeyI,         () => run(onmodi))
    ed.addCommand(CtrlCmd | KeyW,         () => run(onmodw))
    ed.addCommand(CtrlCmd | KeyN,         () => run(onmodn))
    ed.addCommand(CtrlCmd | KeyM,         () => run(onmodm))
    ed.addCommand(CtrlCmd | KeyT,         () => run(onmodt))
    ed.addCommand(CtrlCmd | Shift | KeyD, () => run(onmodshiftd))
    ed.addCommand(CtrlCmd | Alt | KeyD, () => run(onmodaltd))
    ed.addCommand(CtrlCmd | Shift | KeyE, () => run(onmodshifte))
  }

  // ── Valid JS identifier table names only ──────────────────────────────────
  /** @param {string} name */
  function isValidIdentifier(name) {
    return /^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(name);
  }

  function getTableNames() {
    /** @type {string[]} */
    let names = [];
    const hints = /** @type {any} */ (schemaHints);
    if (hints?.tables && Array.isArray(hints.tables)) {
      names = hints.tables;
    } else if (
      hints?.columnsByTable &&
      typeof hints.columnsByTable === "object"
    ) {
      names = Object.keys(hints.columnsByTable).filter(
        (k) => !k.startsWith("__") && !k.includes("."),
      );
    }
    return names.filter(isValidIdentifier);
  }

  function defaultCode() {
    const tables = getTableNames();
    const t = tables[0] ?? "tableName";
    return mode === "prisma"
      ? `prisma.${t}.findMany({ take: 10 })`
      : `db.select().from(${t}).limit(10)`;
  }

  // ── JSON output ───────────────────────────────────────────────────────────
  // Bounded for the same reason as the SQL console's: this builds an object per
  // row and a formatted string of all of them, both O(rows × columns), while
  // the table beside it is virtualised and pays for neither. Export still
  // carries the whole result.
  const JSON_VIEW_ROWS = 1000;

  const jsonRows = $derived(
    rows.length > JSON_VIEW_ROWS ? rows.slice(0, JSON_VIEW_ROWS) : rows,
  );
  const rowObjects = $derived(
    columns.length > 0 && jsonRows.length > 0 ? rowsToObjects(columns, jsonRows) : [],
  );

  const jsonText = $derived(
    rowObjects.length > 0 ? JSON.stringify(rowObjects, null, 2) : "[]",
  );

  // ── Monaco type declarations ──────────────────────────────────────────────

  function buildDrizzleTypes(tableNames, columnsByTable) {
    const lines = [
      `interface DbCol { col: string; table: string; toString(): string; }`,
      `interface SqlCond { __sql: string; }`,
      `interface OrderSpec { __orderBy: { col: string; dir: string }; }`,
      `interface QueryResult { sql: string; params: any[]; }`,
      `interface SelectBuilder {`,
      `  from(table: any): SelectBuilder;`,
      `  where(cond: SqlCond | string): SelectBuilder;`,
      `  orderBy(...args: (OrderSpec | SqlCond | string)[]): SelectBuilder;`,
      `  groupBy(...cols: (DbCol | string)[]): SelectBuilder;`,
      `  having(cond: SqlCond | string): SelectBuilder;`,
      `  limit(n: number): SelectBuilder;`,
      `  offset(n: number): SelectBuilder;`,
      `  leftJoin(table: any, on: SqlCond | string): SelectBuilder;`,
      `  innerJoin(table: any, on: SqlCond | string): SelectBuilder;`,
      `  rightJoin(table: any, on: SqlCond | string): SelectBuilder;`,
      `  fullJoin(table: any, on: SqlCond | string): SelectBuilder;`,
      `  toSQL(): QueryResult;`,
      `}`,
      `interface InsertBuilder {`,
      `  values(data: Record<string, any> | Record<string, any>[]): InsertBuilder;`,
      `  returning(): InsertBuilder;`,
      `  onConflictDoNothing(): InsertBuilder;`,
      `  onConflictDoUpdate(opts: { target: DbCol | DbCol[]; set: Record<string, any> }): InsertBuilder;`,
      `  toSQL(): QueryResult;`,
      `}`,
      `interface UpdateBuilder {`,
      `  set(data: Record<string, any>): UpdateBuilder;`,
      `  where(cond: SqlCond | string): UpdateBuilder;`,
      `  returning(): UpdateBuilder;`,
      `  toSQL(): QueryResult;`,
      `}`,
      `interface DeleteBuilder {`,
      `  where(cond: SqlCond | string): DeleteBuilder;`,
      `  returning(): DeleteBuilder;`,
      `  toSQL(): QueryResult;`,
      `}`,
      `declare const db: {`,
      `  select(cols?: Record<string, DbCol | SqlCond>): SelectBuilder;`,
      `  insert(table: any): InsertBuilder;`,
      `  update(table: any): UpdateBuilder;`,
      `  delete(table: any): DeleteBuilder;`,
      `};`,
      `declare function eq(col: DbCol | string, val: any): SqlCond;`,
      `declare function ne(col: DbCol | string, val: any): SqlCond;`,
      `declare function gt(col: DbCol | string, val: any): SqlCond;`,
      `declare function gte(col: DbCol | string, val: any): SqlCond;`,
      `declare function lt(col: DbCol | string, val: any): SqlCond;`,
      `declare function lte(col: DbCol | string, val: any): SqlCond;`,
      `declare function like(col: DbCol | string, pattern: string): SqlCond;`,
      `declare function ilike(col: DbCol | string, pattern: string): SqlCond;`,
      `declare function notIlike(col: DbCol | string, pattern: string): SqlCond;`,
      `declare function isNull(col: DbCol | string): SqlCond;`,
      `declare function isNotNull(col: DbCol | string): SqlCond;`,
      `declare function inArray(col: DbCol | string, vals: any[]): SqlCond;`,
      `declare function notInArray(col: DbCol | string, vals: any[]): SqlCond;`,
      `declare function between(col: DbCol | string, min: any, max: any): SqlCond;`,
      `declare function notBetween(col: DbCol | string, min: any, max: any): SqlCond;`,
      `declare function and(...conds: (SqlCond | undefined)[]): SqlCond;`,
      `declare function or(...conds: (SqlCond | undefined)[]): SqlCond;`,
      `declare function not(cond: SqlCond): SqlCond;`,
      `declare function asc(col: DbCol | string): OrderSpec;`,
      `declare function desc(col: DbCol | string): OrderSpec;`,
      `declare function count(col?: DbCol | string): SqlCond;`,
      `declare function sum(col: DbCol | string): SqlCond;`,
      `declare function avg(col: DbCol | string): SqlCond;`,
      `declare function max(col: DbCol | string): SqlCond;`,
      `declare function min(col: DbCol | string): SqlCond;`,
      `declare function sql(strings: TemplateStringsArray | string, ...vals: any[]): SqlCond;`,
    ];
    for (const name of tableNames) {
      if (!isValidIdentifier(name)) continue;
      const cols = /** @type {string[]} */ (columnsByTable?.[name] ?? []);
      if (cols.length > 0) {
        // No index signature - named properties give proper autocomplete in Monaco
        lines.push(`declare const ${name}: { ${cols.map((c) => `${c}: DbCol`).join("; ")}; };`);
      } else {
        lines.push(`declare const ${name}: { [col: string]: DbCol; };`);
      }
    }
    return lines.join("\n");
  }

  function buildPrismaTypes(tableNames, columnsByTable) {
    const lines = [
      `interface QueryResult { sql: string; params: any[]; }`,
      `interface StringFilter { equals?: string; contains?: string; startsWith?: string; endsWith?: string; not?: string; in?: string[]; notIn?: string[]; }`,
      `interface NumberFilter { equals?: number; gt?: number; gte?: number; lt?: number; lte?: number; not?: number; in?: number[]; notIn?: number[]; }`,
      `interface OrderByClause { [field: string]: 'asc' | 'desc'; }`,
      `interface AggregateArgs { _count?: true | Record<string, boolean>; _sum?: Record<string, boolean>; _avg?: Record<string, boolean>; _min?: Record<string, boolean>; _max?: Record<string, boolean>; where?: Record<string, any>; }`,
      `interface GroupByArgs { by: string[]; _count?: true | Record<string, boolean>; _sum?: Record<string, boolean>; _avg?: Record<string, boolean>; _min?: Record<string, boolean>; _max?: Record<string, boolean>; where?: Record<string, any>; having?: Record<string, any>; orderBy?: OrderByClause | OrderByClause[]; take?: number; skip?: number; }`,
      `interface PrismaModel {`,
      `  findMany(args?: { where?: Record<string, any>; orderBy?: OrderByClause | OrderByClause[]; take?: number; skip?: number; select?: Record<string, boolean>; cursor?: Record<string, any> }): QueryResult;`,
      `  findFirst(args?: { where?: Record<string, any>; orderBy?: OrderByClause | OrderByClause[]; select?: Record<string, boolean> }): QueryResult;`,
      `  findFirstOrThrow(args?: { where?: Record<string, any>; select?: Record<string, boolean> }): QueryResult;`,
      `  findUnique(args: { where: Record<string, any>; select?: Record<string, boolean> }): QueryResult;`,
      `  findUniqueOrThrow(args: { where: Record<string, any>; select?: Record<string, boolean> }): QueryResult;`,
      `  create(args: { data: Record<string, any>; select?: Record<string, boolean> }): QueryResult;`,
      `  createMany(args: { data: Record<string, any>[]; skipDuplicates?: boolean }): QueryResult;`,
      `  update(args: { data: Record<string, any>; where: Record<string, any>; select?: Record<string, boolean> }): QueryResult;`,
      `  updateMany(args: { data: Record<string, any>; where?: Record<string, any> }): QueryResult;`,
      `  delete(args: { where: Record<string, any>; select?: Record<string, boolean> }): QueryResult;`,
      `  deleteMany(args?: { where?: Record<string, any> }): QueryResult;`,
      `  count(args?: { where?: Record<string, any> }): QueryResult;`,
      `  aggregate(args?: AggregateArgs): QueryResult;`,
      `  groupBy(args: GroupByArgs): QueryResult;`,
      `  upsert(args: { create: Record<string, any>; update: Record<string, any>; where?: Record<string, any> }): QueryResult;`,
      `}`,
    ];
    // Per-model typed interface with column-aware where/select
    for (const name of tableNames) {
      if (!isValidIdentifier(name)) continue;
      const cols = /** @type {string[]} */ ((columnsByTable ?? {})[name] ?? []);
      if (cols.length > 0) {
        const colFields = cols.map((c) => `${c}?: any`).join('; ');
        const selectFields = cols.map((c) => `${c}?: boolean`).join('; ');
        lines.push(
          `interface ${name}Where { ${colFields}; AND?: ${name}Where[]; OR?: ${name}Where[]; NOT?: ${name}Where; }`,
          `interface ${name}Select { ${selectFields}; }`,
          `interface ${name}Model {`,
          `  findMany(args?: { where?: ${name}Where; orderBy?: OrderByClause | OrderByClause[]; take?: number; skip?: number; select?: ${name}Select; cursor?: ${name}Where }): QueryResult;`,
          `  findFirst(args?: { where?: ${name}Where; select?: ${name}Select }): QueryResult;`,
          `  findFirstOrThrow(args?: { where?: ${name}Where; select?: ${name}Select }): QueryResult;`,
          `  findUnique(args: { where: ${name}Where; select?: ${name}Select }): QueryResult;`,
          `  findUniqueOrThrow(args: { where: ${name}Where; select?: ${name}Select }): QueryResult;`,
          `  create(args: { data: Partial<${name}Where>; select?: ${name}Select }): QueryResult;`,
          `  createMany(args: { data: Partial<${name}Where>[]; skipDuplicates?: boolean }): QueryResult;`,
          `  update(args: { data: Partial<${name}Where>; where: ${name}Where; select?: ${name}Select }): QueryResult;`,
          `  updateMany(args: { data: Partial<${name}Where>; where?: ${name}Where }): QueryResult;`,
          `  delete(args: { where: ${name}Where; select?: ${name}Select }): QueryResult;`,
          `  deleteMany(args?: { where?: ${name}Where }): QueryResult;`,
          `  count(args?: { where?: ${name}Where }): QueryResult;`,
          `  aggregate(args?: AggregateArgs): QueryResult;`,
          `  groupBy(args: GroupByArgs): QueryResult;`,
          `  upsert(args: { create: Partial<${name}Where>; update: Partial<${name}Where>; where?: ${name}Where }): QueryResult;`,
          `}`,
        );
      }
    }
    if (tableNames.length > 0) {
      lines.push(`declare const prisma: {`);
      for (const name of tableNames) {
        if (!isValidIdentifier(name)) continue;
        const cols = /** @type {string[]} */ ((columnsByTable ?? {})[name] ?? []);
        lines.push(`  ${name}: ${cols.length > 0 ? `${name}Model` : 'PrismaModel'};`);
      }
      lines.push(`};`);
    } else {
      lines.push(`declare const prisma: { [model: string]: PrismaModel; };`);
    }
    return lines.join("\n");
  }

  /** @type {import('monaco-editor').IDisposable | null} */
  let _extraLibDisposable = null

  function updateMonacoTypes() {
    const tableNames = getTableNames();
    const columnsByTable =
      /** @type {any} */ (schemaHints)?.columnsByTable ?? {};
    const dts =
      mode === "drizzle"
        ? buildDrizzleTypes(tableNames, columnsByTable)
        : buildPrismaTypes(tableNames, columnsByTable);
    try {
      if (_extraLibDisposable) { _extraLibDisposable.dispose(); _extraLibDisposable = null; }
      _extraLibDisposable = monaco.languages.typescript.javascriptDefaults.addExtraLib(dts, "file:///orm-defs.d.ts");
    } catch {
      /* monaco not ready */
    }
  }

  // ── Parse / run ────────────────────────────────────────────────────────────

  function parseQuery() {
    const tableNames = getTableNames();
    const trimmed = code.trim();
    if (!trimmed) {
      parseError = "";
      generatedSql = "";
      return null;
    }
    try {
      const result =
        mode === "drizzle"
          ? evalDrizzleQuery(trimmed, tableNames)
          : evalPrismaQuery(trimmed, tableNames);
      parseError = "";
      generatedSql = result.sql;
      return result;
    } catch (/** @type {any} */ e) {
      parseError = e instanceof Error ? e.message : String(e);
      generatedSql = "";
      return null;
    }
  }

  // Live SQL preview - debounced so typing doesn't compile a new Function and
  // re-highlight the preview on every keystroke. Run/Copy SQL call parseQuery()
  // synchronously themselves, so they never see stale output.
  $effect(() => {
    void code;
    void mode;
    void schemaHints;
    const t = setTimeout(parseQuery, 200);
    return () => clearTimeout(t);
  });

  async function handleRun() {
    const result = parseQuery();
    if (!result) return;
    onrun({ sql: result.sql, mode });
  }

  function handleCopySql() {
    const result = parseQuery();
    if (!result) return;
    copyWithFeedback(result.sql, "sql");
  }

  function handleCopyCode() {
    copyWithFeedback(code, "code");
  }

  function handleCopyError() {
    copyWithFeedback(error, "error");
  }

  function handleCopyParseError() {
    copyWithFeedback(parseError, "parse");
  }

  function copyJson() {
    copyWithFeedback(jsonText, "json");
  }

  $effect(() => {
    void schemaHints;
    void mode;
    updateMonacoTypes();
  });
  $effect(() => {
    codeByMode[mode] = code;
  });

  // Fill default only when mode switches (not on every code change).
  // Using untrack for the code read so clearing the editor stays empty.
  $effect(() => {
    void mode;
    untrack(() => {
      if (!code.trim()) {
        const next = defaultCode();
        code = next;
        if (editor && editor.getValue() !== next) editor.setValue(next);
      }
    });
  });


  const isMac =
    typeof navigator !== "undefined" &&
    navigator.platform.toUpperCase().includes("MAC");
  const mod = isMac ? "⌘" : "Ctrl";

  onMount(() => {
    configureMonacoWorkers();
    defineStrokeMonacoThemes();

    if (!code.trim()) code = defaultCode();

    // JavaScript mode with type declaration injection gives correct ORM completions.
    // TypeScript mode + noLib:false floods the suggestion list with DOM/WebGL types.
    monaco.languages.typescript.javascriptDefaults.setCompilerOptions({
      target: monaco.languages.typescript.ScriptTarget.ES2020,
      allowNonTsExtensions: true,
      noLib: false,
      allowJs: true,
      checkJs: false,
    });
    monaco.languages.typescript.javascriptDefaults.setDiagnosticsOptions({
      noSemanticValidation: true,
      noSyntaxValidation: false,
    });
    updateMonacoTypes();

    if (!container) return;
    const { fontSize, lineHeight } = readEditorFontOptions();

    editor = monaco.editor.create(container, {
      value: code,
      language: "javascript",
      theme: monacoThemeId(currentTheme()),
      // automaticLayout:false - that option polls via setInterval(100ms) forever,
      // even while this tab is hidden. ResizeObserver fires only on real resizes.
      automaticLayout: false,
      minimap: { enabled: false },
      fontFamily: editorFontFamily(),
      fontSize,
      lineHeight,
      fontLigatures: false,
      fontWeight: "normal",
      padding: { top: 12, bottom: 12 },
      scrollBeyondLastLine: false,
      wordWrap: "on",
      renderLineHighlight: "line",
      lineNumbers: "on",
      lineNumbersMinChars: 3,
      glyphMargin: false,
      folding: false,
      scrollbar: { verticalScrollbarSize: 8, horizontalScrollbarSize: 8 },
      overviewRulerLanes: 0,
      hideCursorInOverviewRuler: true,
      overviewRulerBorder: false,
      cursorBlinking: "smooth",
      cursorSmoothCaretAnimation: "on",
      smoothScrolling: true,
      bracketPairColorization: { enabled: true },
      suggest: { showWords: false },
    });

    registerShortcuts(editor);
    editorReady = true;
    const ro = new ResizeObserver(() => editor?.layout());
    ro.observe(container);
    editor.onDidChangeModelContent(() => {
      const next = editor?.getValue() ?? "";
      if (next !== code) code = next;
    });

    return () => {
      ro.disconnect();
      editor?.dispose();
      editor = null;
      // Dispose the Monaco extra-lib - it lives on javascriptDefaults globally.
      if (_extraLibDisposable) { _extraLibDisposable.dispose(); _extraLibDisposable = null; }
      if (copiedTimer) clearTimeout(copiedTimer);
    };
  });

  // Experimental Vim mode - attach monaco-vim (lazy) while enabled.
  $effect(() => {
    const on = $appVimMode;
    const el = vimStatusEl;
    if (!on || !editorReady || !editor || !el) return;
    let disposed = false;
    /** @type {{ dispose: () => void } | null} */
    let inst = null;
    /** @type {MutationObserver | null} */
    let obs = null;
    import("monaco-vim")
      .then(({ initVimMode }) => {
        if (disposed || !editor) return;
        inst = initVimMode(editor, el);
        obs = new MutationObserver(() => {
          const t = el.textContent ?? "";
          setVimSubMode(/INSERT/i.test(t) ? "insert" : /VISUAL/i.test(t) ? "visual" : "normal");
        });
        obs.observe(el, { childList: true, subtree: true, characterData: true });
        setVimSubMode("normal");
      })
      .catch(() => {});
    return () => {
      disposed = true;
      obs?.disconnect();
      inst?.dispose();
    };
  });

  $effect(() => {
    if (!editor) return;
    if (editor.getValue() !== code) editor.setValue(code);
  });
</script>

<div bind:this={consoleEl} class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <!-- ── Toolbar ─────────────────────────────────────────────────────────── -->
  <div
    class="studio-chrome flex h-9 shrink-0 items-center gap-3 border-b border-border bg-panel px-3"
    data-studio-chrome
  >
    <!-- ORM mode -->
    <div class="flex h-7 shrink-0 items-center gap-2">
      <Code2
        class="size-3.5 shrink-0 text-muted-foreground"
        aria-hidden="true"
      />
      <div
        role="tablist"
        aria-label="ORM"
        class="inline-flex h-7 items-center rounded-md bg-muted/40 p-0.5 ring-1 ring-inset ring-border/60"
      >
        <button
          type="button"
          role="tab"
          aria-selected={mode === "drizzle"}
          class={cn(
            "inline-flex h-6 items-center rounded-[5px] px-2.5 font-mono text-ui-xs font-medium transition-all",
            mode === "drizzle"
              ? "bg-card text-foreground ring-1 ring-border/50"
              : "text-muted-foreground hover:text-foreground",
          )}
          onclick={() => switchMode("drizzle")}
        >
          drizzle
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "prisma"}
          class={cn(
            "inline-flex h-6 items-center rounded-[5px] px-2.5 font-mono text-ui-xs font-medium transition-all",
            mode === "prisma"
              ? "bg-card text-foreground ring-1 ring-border/50"
              : "text-muted-foreground hover:text-foreground",
          )}
          onclick={() => switchMode("prisma")}
        >
          prisma
        </button>
      </div>
    </div>

    <Button
      type="button"
      variant="default"
      size="sm"
      class="h-7 shrink-0 gap-1.5 px-2.5 font-medium"
      disabled={loading || !code.trim()}
      title={`Run query (${mod}↵)`}
      onclick={() => void handleRun()}
    >
      {#if loading}
        <Loader2 class="size-3.5 shrink-0 animate-spin" />
      {:else}
        <Play class="size-3.5 shrink-0 fill-current" />
      {/if}
      Run
    </Button>

    <Button
      type="button"
      variant="ghost"
      size="sm"
      class="h-7 shrink-0 px-2.5 font-mono text-ui-xs text-muted-foreground hover:text-foreground"
      disabled={!code.trim()}
      title={`Format code (${mod}S)`}
      onclick={() => editor?.getAction("editor.action.formatDocument")?.run()}
    >
      Format
    </Button>

    <div class="h-4 w-px shrink-0 bg-border/80" aria-hidden="true"></div>

    <div
    class= "field-surface inline-flex h-7 items-center bg-muted/20 p-0.5"
      role="group"
      aria-label="Copy actions"
    >
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="h-6 gap-1.5 rounded-[5px] px-2 text-ui-xs text-muted-foreground hover:bg-background/60 hover:text-foreground"
        disabled={!generatedSql}
        title="Copy generated SQL"
        onclick={handleCopySql}
      >
        {#if copied === "sql"}
          <CheckCheck class="size-3.5 shrink-0 text-success" />
        {:else}
          <Copy class="size-3.5 shrink-0" />
        {/if}
        <span class="text-ui-xs"
          >{copied === "sql" ? "Copied!" : "Copy SQL"}</span
        >
      </Button>
      <div
        class="mx-0.5 h-3.5 w-px shrink-0 bg-border/70"
        aria-hidden="true"
      ></div>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="h-6 gap-1.5 rounded-[5px] px-2 text-ui-xs text-muted-foreground hover:bg-background/60 hover:text-foreground"
        disabled={!code.trim()}
        title="Copy ORM code"
        onclick={handleCopyCode}
      >
        {#if copied === "code"}
          <CheckCheck class="size-3.5 shrink-0 text-success" />
        {:else}
          <Copy class="size-3.5 shrink-0" />
        {/if}
        <span class="text-ui-xs"
          >{copied === "code" ? "Copied!" : "Copy Code"}</span
        >
      </Button>
    </div>

    <div class="ml-auto flex items-center gap-2">
      {#if queryMs > 0}
        <span
          class="shrink-0 rounded-md bg-muted/40 px-1.5 py-0.5 font-mono text-ui-2xs tabular-nums text-muted-foreground ring-1 ring-inset ring-border/40"
        >
          {queryMs}ms
        </span>
      {/if}
    </div>
  </div>

  <!-- ── Monaco editor ─────────────────────────────────────────────────── -->
  <div
    class="relative shrink-0 overflow-hidden border-b border-border bg-panel"
    style="height: {editorHeight}px"
  >
    <div bind:this={container} class="absolute inset-0 h-full w-full overflow-hidden"></div>
    {#if $appVimMode}
      <div
        bind:this={vimStatusEl}
        class="absolute inset-x-0 bottom-0 z-10 border-t border-border/40 bg-panel/95 px-3 py-0.5 font-mono text-ui-2xs leading-5 text-muted-foreground"
      ></div>
    {/if}
  </div>

  <!-- Parse error, inline below editor -->
  {#if parseError}
    <div
      class="group/console-error shrink-0 border-b border-destructive/20 bg-destructive/5 px-3 py-2"
    >
      <div class="flex items-start gap-2">
        <p
          class="min-w-0 flex-1 font-mono text-ui-xs leading-relaxed text-destructive"
        >
          {parseError}
        </p>
        <button
          type="button"
          class="inline-flex h-6 shrink-0 items-center gap-1 rounded px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
          title="Copy error"
          onclick={handleCopyParseError}
        >
          {#if copied === "parse"}
            <CheckCheck
              class="size-3 shrink-0 text-success"
            />
          {:else}
            <Copy class="size-3 shrink-0" />
          {/if}
          <span>{copied === "parse" ? "Copied" : "Copy"}</span>
        </button>
      </div>
    </div>
  {/if}

  <!-- ── Editor resize handle ──────────────────────────────────────────── -->
  <ResizeHandle
    axis="y"
    edge="end"
    onresizestart={() => {
      resizeStartHeight = editorHeight;
    }}
    onresize={(dy) => {
      editorHeight = clampEditorHeight(resizeStartHeight + dy);
    }}
    onresizeend={() => {
      resizeStartHeight = editorHeight;
      saveLayout({ sqlEditorHeight: editorHeight });
    }}
  />

  <!-- ── Generated SQL preview ─────────────────────────────────────────── -->
  <div class="shrink-0 border-b border-border">
    <button
      type="button"
      class="flex w-full items-center gap-1.5 px-3 py-1.5 text-left transition-colors hover:bg-muted/30"
      onclick={() => (sqlPreviewOpen = !sqlPreviewOpen)}
    >
      <ChevronDown
        class={cn(
          "size-3 shrink-0 text-muted-foreground transition-transform duration-150",
          !sqlPreviewOpen && "-rotate-90",
        )}
      />
      <span
        class="font-mono text-ui-2xs font-medium tracking-wide text-muted-foreground uppercase"
        >Generated SQL</span
      >
      {#if generatedSql && !sqlPreviewOpen}
        <span
          class="ml-2 min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground"
          >{generatedSql}</span
        >
      {:else if !generatedSql && !parseError}
        <span class="ml-2 font-mono text-ui-2xs text-muted-foreground"
          >type a query to preview</span
        >
      {/if}
    </button>
    {#if sqlPreviewOpen && generatedSql}
      <div class="px-3 pb-2">
        <ShikiBlock code={generatedSql} lang="sql" embedded class="overflow-x-auto rounded border border-border/60 bg-muted/30" />
      </div>
    {/if}
  </div>

  <!-- ── Results ────────────────────────────────────────────────────────── -->
  <div class="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
    {#if loading}
      <TableLoading />
    {:else if columns.length > 0}
      {#if outputView === "table"}
        <DataTable
          {columns}
          {rows}
          {loading}
          primaryKey={[]}
          foreignKeys={[]}
        />
        <!-- JSON toggle overlay, top right, matching JsonViewer toolbar style -->
        <div
          class="pointer-events-none absolute inset-x-0 top-0 z-10 flex items-start justify-end p-2"
        >
          <button
            type="button"
            onclick={() => (outputView = "json")}
            class="pointer-events-auto inline-flex items-center gap-1.5 rounded-md border border-border/50 bg-background/85 px-2 py-1 font-mono text-ui-2xs text-muted-foreground elevate-2-rim backdrop-blur-sm transition-colors hover:text-foreground"
          >
            <Braces class="size-3 shrink-0" />
            JSON
          </button>
        </div>
      {:else}
        <JsonViewer
          json={jsonText}
          data={rowObjects}
          shownRows={rowObjects.length}
          rowCount={rows.length}
          onshowtable={() => (outputView = "table")}
        />
      {/if}
    {:else if error}
      <div
        class="group/console-error min-h-0 flex-1 overflow-auto border-t border-destructive/15 bg-destructive/[0.03]"
      >
        <div class="flex items-start gap-2 px-3 py-2.5">
          <pre
            class="min-w-0 flex-1 whitespace-pre-wrap font-mono text-ui-xs leading-relaxed text-destructive">{error}</pre>
          <button
            type="button"
            class="inline-flex h-6 shrink-0 items-center gap-1 rounded px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
            title="Copy error"
            onclick={handleCopyError}
          >
            {#if copied === "error"}
              <CheckCheck
                class="size-3 shrink-0 text-success"
              />
            {:else}
              <Copy class="size-3 shrink-0" />
            {/if}
            <span>{copied === "error" ? "Copied" : "Copy"}</span>
          </button>
        </div>
      </div>
    {:else}
      <div
        class="flex h-full flex-col items-center justify-center gap-2 text-center"
      >
        <Play class="size-6 text-muted-foreground" />
        <p class="font-mono text-ui-sm text-muted-foreground">
          Run a query to see results
        </p>
        <p class="font-mono text-ui-xs text-muted-foreground">
          {mode === "drizzle"
            ? `db.select().from(users).limit(10)`
            : `prisma.users.findMany({ take: 10 })`}
        </p>
      </div>
    {/if}
  </div>
</div>

<style>
  div :global(.monaco-editor),
  div :global(.monaco-editor .margin),
  div :global(.monaco-editor-background) {
    border-radius: inherit;
  }

  div :global(.monaco-editor .view-lines),
  div :global(.monaco-editor .view-line) {
    font-weight: 400 !important;
  }

  div :global(.monaco-editor .monaco-editor-background) {
    outline: none !important;
  }

</style>
