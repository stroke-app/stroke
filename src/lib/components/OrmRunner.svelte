<script>
  import { onMount, untrack } from "svelte";
  import { Prec } from "@codemirror/state";
  import { tooltips } from "@codemirror/view";
  import { autocompletion, closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
  import { indentRange } from "@codemirror/language";
  import { keymap } from "@codemirror/view";
  import CodeEditor from "./CodeEditor.svelte";
  import { ormCompletionSource } from "$lib/cm-orm-complete.js";
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
  let consoleEl = $state(null);
  /** @type {CodeEditor | null} */
  let editor = $state(null);

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


  /** @param {'drizzle' | 'prisma'} newMode */
  function switchMode(newMode) {
    if (newMode === mode) return;
    codeByMode[mode] = code;
    mode = newMode;
    code = codeByMode[newMode];
    // If code is empty, the defaultCode $effect will fill it in
  }

  /** Re-indent the whole script by the language's rules (⌘S, as Format was). @param {import('@codemirror/view').EditorView} view */
  function reindent(view) {
    const changes = indentRange(view.state, 0, view.state.doc.length);
    if (!changes.empty) view.dispatch({ changes, userEvent: "input.indent" });
    return true;
  }

  /** @param {(() => void) | undefined} fn */
  const call = (fn) => { fn?.(); return true; };

  /**
   * The editor's own chords, and the app's: a handled chord stops at the
   * editor, so the global shortcuts it shadows are passed on by hand.
   * @type {import('@codemirror/view').KeyBinding[]}
   */
  const keys = [
    { key: "Mod-Enter", run: () => { void handleRun(); return true; } },
    { key: "Mod-s", run: reindent },
    { key: "Mod-i", run: () => call(onmodi) },
    { key: "Mod-w", run: () => call(onmodw) },
    { key: "Mod-n", run: () => call(onmodn) },
    { key: "Mod-m", run: () => call(onmodm) },
    { key: "Mod-t", run: () => call(onmodt) },
    { key: "Mod-Shift-d", run: () => call(onmodshiftd) },
    { key: "Mod-Alt-d", run: () => call(onmodaltd) },
    { key: "Mod-Shift-e", run: () => call(onmodshifte) },
  ];

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

  // ── Completion ────────────────────────────────────────────────────────────
  /** Column names per table, from the hints (plain names or `{ name }` entries, short or schema-qualified keys). */
  function columnsByName() {
    /** @type {Record<string, string[]>} */
    const out = {};
    for (const [key, cols] of Object.entries(/** @type {any} */ (schemaHints)?.columnsByTable ?? {})) {
      if (key === "__result__" || !Array.isArray(cols)) continue;
      const short = key.includes(".") ? (key.split(".").pop() ?? key) : key;
      out[short] ??= cols.map((c) => (typeof c === "string" ? c : c?.name)).filter(Boolean);
    }
    return out;
  }

  /** What the completion reads per query: the mode, the tables and their columns. */
  const ormModel = () => ({
    mode: mode === "drizzle" ? /** @type {const} */ ("drizzle") : /** @type {const} */ ("prisma"),
    tables: getTableNames(),
    columns: columnsByName(),
    loadColumns: /** @type {any} */ (schemaHints)?.loadColumns,
  });

  const completion = [
    autocompletion({ override: [ormCompletionSource(ormModel)] }),
    closeBrackets(),
    keymap.of(closeBracketsKeymap),
    // The list is drawn on <body>: the editor pane clips its overflow.
    tooltips({ parent: document.body }),
  ];

  // Experimental Vim mode: @replit/codemirror-vim, loaded only while it is on.
  let vimExtension = $state(/** @type {import('@codemirror/state').Extension | null} */ (null));
  const editorExtensions = $derived([...(vimExtension ? [Prec.highest(vimExtension)] : []), ...completion]);

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
    codeByMode[mode] = code;
  });

  // Fill default only when mode switches (not on every code change).
  // Using untrack for the code read so clearing the editor stays empty.
  $effect(() => {
    void mode;
    untrack(() => {
      if (!code.trim()) {
        code = defaultCode();
      }
    });
  });


  const isMac =
    typeof navigator !== "undefined" &&
    navigator.platform.toUpperCase().includes("MAC");
  const mod = isMac ? "⌘" : "Ctrl";

  onMount(() => {
    if (!code.trim()) code = defaultCode();
    return () => {
      if (copiedTimer) clearTimeout(copiedTimer);
    };
  });

  $effect(() => {
    if (!$appVimMode) { vimExtension = null; return; }
    let cancelled = false;
    import("@replit/codemirror-vim")
      .then(({ vim }) => { if (!cancelled) vimExtension = vim({ status: true }); })
      .catch(() => {});
    return () => { cancelled = true; };
  });

  // Mirror Vim's mode into the shared status-bar indicator.
  $effect(() => {
    const ext = vimExtension;
    const view = editor?.getView();
    if (!ext || !view) return;
    /** @type {any} */
    let cm = null;
    /** @param {{ mode: string }} e */
    const onMode = (e) => setVimSubMode(e.mode === "insert" ? "insert" : e.mode === "visual" ? "visual" : "normal");
    // A frame later: CodeEditor installs the extension in its own effect.
    import("@replit/codemirror-vim").then(({ getCM }) => requestAnimationFrame(() => {
      cm = getCM(view);
      cm?.on("vim-mode-change", onMode);
      setVimSubMode("normal");
    }));
    return () => cm?.off("vim-mode-change", onMode);
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

  <!-- ── Editor ────────────────────────────────────────────────────────── -->
  <div
    class="relative flex shrink-0 flex-col overflow-hidden border-b border-border bg-panel"
    style="height: {editorHeight}px"
    data-vim-editor={$appVimMode ? '' : undefined}
  >
    <CodeEditor
      bind:this={editor}
      bind:value={code}
      lang="javascript"
      {keys}
      extensions={editorExtensions}
      folding={false}
      ariaLabel="{mode === 'drizzle' ? 'Drizzle' : 'Prisma'} query"
    />
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
