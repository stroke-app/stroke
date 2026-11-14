<script>
  import CollapsibleCode from './CollapsibleCode.svelte'
  import { tick, onMount, onDestroy } from "svelte";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import CornerDownRight from "@lucide/svelte/icons/corner-down-right";
  import Reply from "@lucide/svelte/icons/reply";
  import Square from "@lucide/svelte/icons/square";
  import Settings2 from "@lucide/svelte/icons/settings-2";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Play from "@lucide/svelte/icons/play";
  import PenLine from "@lucide/svelte/icons/pen-line";
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import AlertTriangle from "@lucide/svelte/icons/alert-triangle";
  import Globe from "@lucide/svelte/icons/globe";
  import Table2 from "@lucide/svelte/icons/table-2";
  import MessageSquare from "@lucide/svelte/icons/message-square";
  import Zap from "@lucide/svelte/icons/zap";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import Icon from "./Icon.svelte";
  import X from "@lucide/svelte/icons/x";
  import Database from "@lucide/svelte/icons/database";
  import Lock from "@lucide/svelte/icons/lock";
  import Upload from "@lucide/svelte/icons/upload";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import BarChart2 from "@lucide/svelte/icons/bar-chart-2";
  import Cpu from "@lucide/svelte/icons/cpu";
  import Maximize2 from "@lucide/svelte/icons/maximize-2";
  import Minimize2 from "@lucide/svelte/icons/minimize-2";
  import ResizeHandle from "$lib/components/ResizeHandle.svelte";
  import { getAppScale } from "$lib/app-zoom.js";
  import ArrowDownToLine from "@lucide/svelte/icons/arrow-down-to-line";
  import ZoomIn from "@lucide/svelte/icons/zoom-in";
  import ZoomOut from "@lucide/svelte/icons/zoom-out";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { cn } from "$lib/utils.js";
  import { executeSql, executeSqlOnConnection, aiWebSearch, aiFetchPage, saveExportAs, agentStart, agentSend, agentInterrupt, agentStop } from "$lib/api.js";
  import { queryLanguageFor, sampleQuery, describeFromSample } from "$lib/ai-query-languages.js";
  import {
    profileAgent,
    strokeToolName,
    toolOutputText,
    parseSqlResult,
    agentSystemPrompt,
    isWebTool,
    agentPrefs,
    AGENTS_WITH_EFFORT,
    stepLabel,
    cleanToolError,
  } from "$lib/local-agents.js";
  import { extractPlan, sqlStepLabel, groupRetries, retryLabel, planProgress, summarizeSteps, formatStepDuration } from "$lib/ai-steps.js";
  import AgentComposerChips from "$lib/components/AgentComposerChips.svelte";
  import { isReadOnly, guardWrite, readOnlyMode } from "$lib/stores/read-only.js";
  import { isWriteSql, isWriteFor, stripSqlComments } from "$lib/sql-write.js";
  import {
    rowsToCsv,
    rowsToJson,
    rowsToMarkdown,
    rowsToCsvAsync,
    rowsToJsonAsync,
    saveExportFile,
    buildExportFilename,
  } from "$lib/export.js";
  import DataTable from "$lib/components/DataTable.svelte";
  import AiMarkdown from "$lib/components/AiMarkdown.svelte";
  import AiSqlBlock from "$lib/components/AiSqlBlock.svelte";
  import ShikiBlock from "$lib/components/ShikiBlock.svelte";
  import AiChartRenderer from "$lib/components/AiChartRenderer.svelte";
  import {
    chatCompletionStream,
    manageHistory,
    MAX_AI_RETRIES,
    AI_TOOLS,
    AI_WEB_TOOLS,
    isDestructiveSql,
    parseAssistantMessage,
    buildSystemPrompt,
    classifyDbError,
    describeAiError,
    humanizeDbError,
    filterSchemaForQuery,
    detectPromptTopics,
    toolsForTurn,
    conversationTitle,
    isPlaceholderTitle,
    historyBudget,
    repairToolPairs,
    extractTextToolCalls,
  } from "$lib/ai.js";
  import { chartRows } from "$lib/ai-chart-data.js";
  import {
    loadSkills,
    saveSkills,
    parseSkillFile,
  } from "$lib/stores/ai-skills.js";
  import { renderMermaidSync } from "beautiful-mermaid";
  import { normalizeThemeId } from "$lib/themes/registry.js";
  import { liveMermaidTheme } from "$lib/mermaid-theme.js";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import {
    aiSettings,
    aiProfiles,
    activeProfileId,
    modelDisplayName,
  } from "$lib/stores/ai-settings.js";
  import BrandIcon from "$lib/components/BrandIcon.svelte";
  import { hasBrand } from "$lib/brand-icons.js";
  import {
    aiChatParams,
    updateChatParams,
    resetChatParams,
  } from "$lib/stores/ai-chat-params.js";
  import { saveChart } from "$lib/stores/saved-charts.js";
  import { saveDiagram } from "$lib/stores/saved-diagrams.js";
  import { buildOption } from "$lib/chart-utils.js";
  import { isCurrentThemeDark, appAgentQueryCards, appAgentWebAccess, toggleLightDark } from "$lib/stores/settings.js";
  import Save from "@lucide/svelte/icons/save";
  import Check from "@lucide/svelte/icons/check";
  import Search from "@lucide/svelte/icons/search";
  import Layers from "@lucide/svelte/icons/layers";
  import Hash from "@lucide/svelte/icons/hash";
  import Clock from "@lucide/svelte/icons/clock";
  import HardDrive from "@lucide/svelte/icons/hard-drive";
  import Link2 from "@lucide/svelte/icons/link-2";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Circle from "@lucide/svelte/icons/circle";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import BarChart3 from "@lucide/svelte/icons/chart-column";
  import ImageIcon from "@lucide/svelte/icons/image";
  import { loadLayout, setNavSidebarWidth, clampNavSidebarWidth, NAV_SIDEBAR_MIN, NAV_WIDTH_EVENT } from "$lib/stores/layout.js";
  import Brain from "@lucide/svelte/icons/brain";
  import AiModelPicker from "./AiModelPicker.svelte";
  import { publishChatList, aiChatPending, takeChatRequest } from "$lib/stores/ai-chat-bridge.js";
  import {
    listConversations,
    createConversation,
    updateConversation,
    deleteConversation,
  } from "$lib/stores/conversations.js";
  import { generateSuggestions } from "$lib/ai-suggestions.js";
  import { formatCompactCount } from "$lib/table-list.js";
  import { svgToPngBlob, downloadBlob, canvasToPngBlob } from "$lib/svg-png.js";
  import { focusTrap } from "$lib/actions/focus-trap.js";

  /**
   * @typedef {
   *   | { id: string, kind: 'user', text: string, ts?: number }
   *   | { id: string, kind: 'assistant', parts: import('$lib/ai.js').AssistantPart[], ts?: number }
   *   | { id: string, kind: 'streaming' }
   *   | { id: string, kind: 'generating', what: 'chart' | 'diagram' }
   *   | { id: string, kind: 'result', sql: string, columns: {name:string,dataType?:string}[], rows: unknown[][], total: number, error: string|null, isSchema?: boolean, capped?: boolean }
   *   | { id: string, kind: 'chart', spec: { type: string, title: string, data: object[], x_col: string, y_col: string, z_col?: string, group_col?: string }, error: string|null }
   *   | { id: string, kind: 'confirm', sql: string, resolve: (ok: boolean) => void }
   *   | { id: string, kind: 'thinking' }
   *   | { id: string, kind: 'executing', sql: string, op: 'query' | 'schema' | 'describe' | 'run' | 'diagram' }
   *   | { id: string, kind: 'diagram', code: string, title: string }
   *   | { id: string, kind: 'web', mode: 'search'|'fetch', label: string, hits: {title:string,url:string,snippet:string}[], error: string|null, loading: boolean }
   *   | { id: string, kind: 'work', startedAt: number, endedAt: number|null, steps: WorkStep[], plan?: string[] }
   * } ChatItem
   */

  /**
   * One step of a local agent's turn, folded under "Worked for 4s": a tool
   * call, or a `note` (what the agent said between calls).
   * The built-in model's steps also carry timing and the affected-row count.
   * @typedef {{ id: string, tool: string, op: 'query' | 'schema' | 'describe' | 'web' | 'note', detail: string, status: 'running' | 'done' | 'error', error?: string, startedAt?: number, endedAt?: number, affected?: number | null, message?: string | null }} WorkStep
   */

  /**
   * Leading keyword of a statement, for the SQL block badge. More useful than a
   * generic "SQL" chip: at a glance you know whether the model is about to read or
   * to write, which is the only question that matters before hitting Run.
   * @param {string} sql
   */
  function sqlStatementKind(sql) {
    const m = /^\s*([a-z]+)/i.exec(stripSqlComments(sql).trim());
    return m ? m[1].toUpperCase().slice(0, 12) : "SQL";
  }

  /**
   * Returns human-readable label + detail for an executing item.
   * @param {'query'|'schema'|'describe'|'run'} op
   * @param {string} sql
   */
  function execMeta(op, sql) {
    if (op === "schema") {
      return {
        label: "Reading schema",
        detail: sql === "all tables" ? "all tables" : sql,
        verb: null,
      };
    }
    if (op === "describe") {
      return { label: "Describing table", detail: sql, verb: null };
    }
    if (op === "diagram") {
      return { label: "Creating diagram", detail: sql, verb: null };
    }
    const trimmed = sql.trim();
    const verb = trimmed.split(/\s+/)[0]?.toUpperCase() ?? "SQL";
    const tableMatch = trimmed.match(
      /\b(?:FROM|INTO|UPDATE|TABLE|JOIN)\s+(?:["'`]?)(\w+)/i,
    );
    const table = tableMatch?.[1] ?? "";
    const labelMap = {
      SELECT: "Querying",
      INSERT: "Inserting",
      UPDATE: "Updating",
      DELETE: "Deleting",
      CREATE: "Creating",
      DROP: "Dropping",
      ALTER: "Altering",
      WITH: "Querying",
    };
    const label = labelMap[verb] ?? "Executing";
    return { label, detail: table || verb, verb };
  }

  let {
    schemaContext = {
      schemas: [],
      activeSchema: "public",
      tables: [],
      activeTable: null,
      columns: [],
      primaryKey: [],
      foreignKeys: [],
    },
    connectionId = "",
    /** Which stored chats are this connection's, older keys included (lib/ai-chat-key.js). @type {((key: string) => boolean) | null} */
    chatMatch = null,
    /** The database the session is in, saved on each chat with its schema. */
    database = "",
    /** The live connection's config: execute_sql's `database` argument runs on a copy of it pointed at that database. @type {Record<string, any> | null} */
    connectionConfig = null,
    isActive = false,
    /** 'tab' = embedded in tab, 'full' = fullscreen AI mode */
    mode = /** @type {'tab' | 'full'} */ ("tab"),
    /** Callback to exit fullscreen AI mode (null when in tab mode) */
    onexit = /** @type {(() => void) | null} */ (null),
    /** App-wide controls for the chat list's footer (the table sidebar's set). */
    onopensettings = /** @type {(() => void) | null} */ (null),
    onopenshortcuts = /** @type {(() => void) | null} */ (null),
    /** The window's own sidebar button drives the chat list (AI mode): no second toggle in here. */
    externalToggle = false,
    /** AI in a tab (beta): the app's sidebar lists the chats, so this hides its own list. */
    listInSidebar = false,
    /** @param {string} sql */
    onwritesql = (sql) => {},
    /** Open the dedicated AI model settings dialog. */
    onopenmodelsettings = () => {},
    /** Called when user saves a diagram - opens Diagrams page */
    onopendiagramspage = /** @type {(() => void) | undefined} */ (undefined),
    /** The AI changed the schema (CREATE / ALTER / DROP...): reload the tables and database list. */
    onschemadirty = () => {},
  } = $props();

  /** A statement that changes the schema, so the sidebar must reload. @param {string} sql */
  const isDdl = (sql) => /^\s*(?:--[^\n]*\n\s*)*(create|alter|drop|rename|truncate|comment\s+on)\b/i.test(sql ?? "") || /;\s*(create|alter|drop|rename|truncate)\b/i.test(sql ?? "");

  $effect(() => {
    if (isActive) {
      // Re-focus the input whenever the AI view becomes visible. A frame later,
      // not a microtask: the view un-hides in the same update, and a hidden
      // (display: none) textarea refuses focus - so entering AI mode left the
      // composer unfocused.
      void tick().then(() => requestAnimationFrame(() => inputRef?.focus({ preventScroll: true })));
    }
  });

  // ── Settings ──────────────────────────────────────────────────────────────
  /** Model config merged with chat params (temperature, topK, maxTokens). */
  const settings = $derived({ ...$aiSettings, ...$aiChatParams });

  /**
   * Save a chart canvas as PNG through the native dialog.
   * Both call sites used to build an `<a download>`, which WKWebView ignores -
   * the button looked like it worked and produced no file.
   * @param {string} selector CSS selector for the chart's canvas
   * @param {string} title used for the default filename
   */
  async function saveChartPng(selector, title) {
    const canvas = /** @type {HTMLCanvasElement|null} */ (document.querySelector(selector));
    const blob = await canvasToPngBlob(canvas);
    if (!blob) {
      toast.error("Could not export the chart");
      return;
    }
    try {
      const path = await saveExportAs(blob, `${title || "chart"}.png`, {
        name: "PNG",
        extensions: ["png"],
      });
      if (!path) return; // save dialog cancelled
      toast.success("Chart saved", { description: `Saved to ${path}` });
    } catch (e) {
      toast.error("Could not save the chart", { description: String(e) });
    }
  }

  /** Open a search result in the user's browser, never inside the app webview. */
  async function openExternal(/** @type {string} */ url) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      toast.error("Could not open the link", { description: url });
    }
  }
  /** Tools advertised this turn. Web tools appear only once the user opts in. */
  /** The tools the current turn advertises - see toolsForTurn. Starts at the
   *  core set so the context stats have something to count before a turn. */
  let turnTools = $state(toolsForTurn(new Set()));
  let settingsOpen = $state(false);
  /** @type {'model'|'skills'|'context'|'chat'} */
  let settingsTab = $state("model");
  const SETTINGS_TABS = /** @type {const} */ ([
    { id: "model", icon: Cpu, label: "Model" },
    { id: "chat", icon: MessageSquare, label: "Chat" },
    { id: "skills", icon: Zap, label: "Skills" },
    { id: "context", icon: Database, label: "Context" },
  ]);

  // ── Skills ────────────────────────────────────────────────────────────────
  /** @type {import('$lib/stores/ai-skills.js').AiSkill[]} */
  let skills = $state(loadSkills());

  let newSkillOpen = $state(false);
  let newSkillName = $state("");
  let newSkillDesc = $state("");
  let newSkillContent = $state("");

  function createSkill() {
    const name = newSkillName.trim();
    const content = newSkillContent.trim();
    if (!name || !content) return;
    const skill = {
      id: crypto.randomUUID(),
      name,
      description: newSkillDesc.trim(),
      content,
    };
    skills = [...skills, skill];
    saveSkills(skills);
    newSkillOpen = false;
    newSkillName = "";
    newSkillDesc = "";
    newSkillContent = "";
  }

  function removeSkill(/** @type {string} */ id) {
    skills = skills.filter((s) => s.id !== id);
    saveSkills(skills);
  }

  /** @param {Event} e */
  function handleSkillFileUpload(e) {
    const files = /** @type {HTMLInputElement} */ (e.target).files;
    if (!files?.length) return;
    Array.from(files).forEach((file) => {
      const reader = new FileReader();
      reader.onload = (ev) => {
        const content = /** @type {string} */ (ev.target?.result ?? "");
        const skill = parseSkillFile(file.name, content);
        skills = [...skills, skill];
        saveSkills(skills);
      };
      reader.readAsText(file);
    });
    /** @type {HTMLInputElement} */ (e.target).value = "";
  }

  // ── Conversations list (IndexedDB) ─────────────────────────────────────────
  /** @type {import('$lib/stores/conversations.js').Conversation[]} */
  let convList = $state([]);
  /** @type {string | null} */
  let activeConvId = $state(null);

  async function loadConvList() {
    convList = await listConversations(chatMatch ?? (connectionId || undefined));
  }

  /** Engines where execute_sql's `database` argument can open another database by name. */
  const CROSS_DB_ENGINES = new Set(["postgres", "cockroachdb", "mysql", "mariadb", "mssql", "clickhouse"]);

  // ── The sidebar's copy of the list (AI in a tab) ────────────────────────────
  /** "shop · public": where a chat last worked. @param {any} c */
  function chatWhere(c) {
    return [c.database, c.schema].filter(Boolean).join(" · ");
  }
  $effect(() => {
    if (!listInSidebar) return;
    publishChatList({
      connectionId,
      chats: convList.map((c) => ({ id: c.id, title: displayTitle(c), updatedAt: c.updatedAt, where: chatWhere(c) })),
      activeId: activeConvId,
      busyId: loading ? activeConvId ?? "draft" : null,
      live: true,
    });
  });
  onDestroy(() => {
    if (listInSidebar) publishChatList({ live: false, busyId: null });
  });

  /** Open or start the chat the sidebar asked for. */
  $effect(() => {
    if (!listInSidebar || !$aiChatPending) return;
    const req = takeChatRequest();
    if (!req) return;
    void (async () => {
      if (req.type === "new") return newConversation();
      if (!convList.some((c) => c.id === req.id)) await loadConvList();
      await selectConversation(req.id);
    })();
  });

  // ── Open conversation tabs (browser-style, persisted per connection) ───────
  const AI_TABS_KEY = "stroke:ai-chat-tabs";

  /** @returns {Record<string, string[]>} */
  function loadTabsMap() {
    try {
      const raw = localStorage.getItem(AI_TABS_KEY);
      if (!raw) return {};
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch {
      return {};
    }
  }

  /**
   * @param {string} connId
   * @returns {string[]}
   */
  function loadOpenTabs(connId) {
    const all = loadTabsMap();
    const v = all[connId || "_default"];
    return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
  }

  /**
   * @param {string} connId
   * @param {string[]} ids
   */
  function saveOpenTabs(connId, ids) {
    try {
      const all = loadTabsMap();
      const key = connId || "_default";
      if (ids.length) all[key] = ids;
      else delete all[key];
      localStorage.setItem(AI_TABS_KEY, JSON.stringify(all));
    } catch {
      // Quota/serialization failure must not throw into the chat flow.
    }
  }

  /** IDs of conversations currently shown as open tabs (a subset of convList). */
  let openTabIds = $state(/** @type {string[]} */ ([]));

  /** Resolve open tab ids to their conversation records, dropping any stale ids. */
  const openTabs = $derived(
    openTabIds
      .map((id) => convList.find((c) => c.id === id))
      .filter((c) => Boolean(c)),
  );

  /** Append a conversation id to the open-tab set if it isn't already shown. */
  function ensureOpenTab(/** @type {string | null} */ id) {
    if (!id || openTabIds.includes(id)) return;
    openTabIds = [...openTabIds, id];
    saveOpenTabs(connectionId, openTabIds);
  }

  /** Close an open tab; when it was active, fall back to the nearest tab or a fresh draft. */
  function closeTab(/** @type {string} */ id) {
    const idx = openTabIds.indexOf(id);
    if (idx === -1) return;
    const next = openTabIds.filter((t) => t !== id);
    openTabIds = next;
    saveOpenTabs(connectionId, next);
    if (id === activeConvId) {
      const fallback = next[idx] ?? next[next.length - 1] ?? null;
      if (fallback) void selectConversation(fallback);
      else void newConversation();
    }
  }

  // A conversation that becomes active (selected, or a draft saved to a real id)
  // is surfaced as an open tab. ensureOpenTab is a no-op once the id is present,
  // so this settles after a single pass and never loops.
  $effect(() => {
    if (activeConvId) ensureOpenTab(activeConvId);
  });

  async function selectConversation(/** @type {string} */ id) {
    if (id === activeConvId) return;
    abortCurrentRequest();
    // Save current before switching
    await persistCurrent();
    const conv = convList.find((c) => c.id === id);
    if (!conv) return;
    activeConvId = id;
    clearAnchor();
    queued = [];
    enterFrom = Number.MAX_SAFE_INTEGER;
    stopAgentSession();
    agentNative = /** @type {any} */ (conv).agentNative ?? null;
    // Restore - filter out ephemeral items (including any stuck streaming items)
    items = /** @type {ChatItem[]} */ (
      (conv.items ?? []).filter(
        (i) =>
          /** @type {any} */ (i).kind !== "thinking" &&
          /** @type {any} */ (i).kind !== "confirm" &&
          /** @type {any} */ (i).kind !== "streaming" &&
          /** @type {any} */ (i).kind !== "executing",
      )
    );
    apiHistory = /** @type {import('$lib/ai.js').ApiMessage[]} */ (
      conv.apiHistory ?? []
    );
    rawApiHistory = /** @type {import('$lib/ai.js').ApiMessage[]} */ (
      conv.apiHistory ?? []
    );
    syncSeq(items);
    enterFrom = items.length;
    error = "";
    await scrollBottom();
  }

  async function newConversation() {
    abortCurrentRequest();
    await persistCurrent();
    activeConvId = null;
    clearAnchor();
    queued = [];
    stopAgentSession();
    agentNative = null;
    items = [];
    enterFrom = 0;
    apiHistory = [];
    rawApiHistory = [];
    error = "";
    await tick();
    inputRef?.focus();
  }

  /** Parse a raw error string into a human-friendly message. */
  $effect(() => {
    void error;
    errorDetailOpen = false;
  });

  async function removeConversation(/** @type {string} */ id) {
    closeContextMenu();
    await deleteConversation(id);
    convList = convList.filter((c) => c.id !== id);
    if (openTabIds.includes(id)) {
      openTabIds = openTabIds.filter((t) => t !== id);
      saveOpenTabs(connectionId, openTabIds);
    }
    if (activeConvId === id) {
      activeConvId = null;
      clearAnchor();
      stopAgentSession();
      agentNative = null;
      items = [];
      apiHistory = [];
      rawApiHistory = [];
      error = "";
    }
  }

  // ── Context menu ───────────────────────────────────────────────────────────
  /** @type {{ id: string, x: number, y: number } | null} */
  let contextMenu = $state(null);

  function showContextMenu(
    /** @type {string} */ id,
    /** @type {MouseEvent} */ e,
  ) {
    e.preventDefault();
    e.stopPropagation();
    contextMenu = { id, x: e.clientX, y: e.clientY };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  /** Persist current chat to IndexedDB (upsert). Updates convList in-place - no re-sort. */
  async function persistCurrent() {
    if (items.length === 0) return;
    const saveable = items.filter(
      (i) =>
        i.kind !== "thinking" && i.kind !== "confirm" && i.kind !== "executing",
    );
    if (saveable.length === 0) return;
    const firstUser = saveable.find((i) => i.kind === "user");
    const title =
      firstUser?.kind === "user"
        ? firstUser.text.slice(0, 60) + (firstUser.text.length > 60 ? "…" : "")
        : "Conversation";
    const plainItems = $state.snapshot(saveable);
    const plainHistory = $state.snapshot(rawApiHistory);
    if (activeConvId) {
      // The title is deliberately NOT written here. It is seeded from the first
      // message on create, then owned by generateAiTitle (and by rename) - this
      // save runs after every turn, so re-deriving it from the first message
      // clobbered the generated title moments after it landed, which is why every
      // conversation in the list stayed named after its opening word.
      // Where it last worked: the chat follows the connection across databases.
      const where = { schema: schemaContext.activeSchema, database };
      await updateConversation(activeConvId, {
        items: plainItems,
        apiHistory: plainHistory,
        agentNative: agentNative ? { ...agentNative } : null,
        ...where,
      });
      convList = convList.map((c) => (c.id === activeConvId ? { ...c, ...where, updatedAt: Date.now() } : c));
    } else {
      const conv = await createConversation({
        title,
        schema: schemaContext.activeSchema,
        database,
        connectionId,
        items: plainItems,
        apiHistory: plainHistory,
        agentNative: agentNative ? { ...agentNative } : null,
      });
      activeConvId = conv.id;
      // Prepend new conversation at the top (it is the newest)
      convList = [conv, ...convList];
    }
  }

  // ── AI-generated title ────────────────────────────────────────────────────
  /** Generate a short title from the conversation and update the stored title. */
  async function generateAiTitle() {
    if (!activeConvId) return;
    const conv = convList.find((c) => c.id === activeConvId);
    // A real title stays: one the user typed, or one already taken from a
    // message with content. Only a placeholder ("hi") is replaced.
    if (conv?.title && !isPlaceholderTitle(conv.title)) return;
    const userTexts = rawApiHistory.filter((m) => m.role === "user").map((m) => String(m.content ?? ""));
    if (!userTexts.length) return;
    // From the messages, not from a second model call: on the free tier that
    // extra request per turn is what tripped the rate limit and the retry wait
    // that followed.
    const title = conversationTitle(userTexts, conv?.createdAt);
    if (title.length < 3 || title === conv?.title) return;
    try {
      await updateConversation(activeConvId, { title });
      convList = convList.map((c) =>
        c.id === activeConvId ? { ...c, title } : c,
      );
    } catch {
      /* non-critical - leave existing title */
    }
  }

  // ── Rename conversation ───────────────────────────────────────────────────
  /** @type {string|null} */
  let renamingConvId = $state(null);

  // ── Selecting chats (multi-delete, clear all) ───────────────────────────────
  let selecting = $state(false);
  /** Anchor for Shift-click range selection: the last chat row clicked. @type {string | null} */
  let convAnchor = null;
  /** @type {Set<string>} */
  let selectedConvs = $state(new Set());
  /** Two-step "Clear all": the first click arms it, the second deletes. */
  let confirmClearAll = $state(false);
  /** @param {string} id */
  function toggleConvSelected(id) {
    const next = new Set(selectedConvs);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedConvs = next;
  }
  function endSelecting() {
    selecting = false;
    selectedConvs = new Set();
    confirmClearAll = false;
  }
  /** @param {string[]} ids */
  async function removeConversations(ids) {
    for (const id of ids) await removeConversation(id);
    endSelecting();
  }
  let renamingTitle = $state("");

  function startRename(
    /** @type {string} */ id,
    /** @type {string} */ currentTitle,
  ) {
    renamingConvId = id;
    renamingTitle = currentTitle;
  }

  async function commitRename() {
    const id = renamingConvId;
    const title = renamingTitle.trim();
    renamingConvId = null;
    if (!id || !title) return;
    await updateConversation(id, { title });
    convList = convList.map((c) => (c.id === id ? { ...c, title } : c));
  }

  function cancelRename() {
    renamingConvId = null;
  }

  // ── Platform ───────────────────────────────────────────────────────────────
  const isMac =
    typeof navigator !== "undefined" &&
    /Mac|iPhone|iPad/i.test(navigator.platform);
  const modKey = isMac ? "⌘" : "Ctrl";
  const newChatShortcut = $derived(`${modKey}⇧T`);

  /** Reset an ECharts instance by its data-chart-id container */
  async function resetChartView(chartId) {
    const container = document.querySelector(`[data-chart-id="${chartId}"]`);
    if (!container) return;
    const { getInstanceByDom } = await import("echarts");
    const canvas = container.querySelector("canvas");
    const chartEl = canvas?.parentElement;
    if (chartEl) {
      getInstanceByDom(chartEl)?.dispatchAction({ type: "restore" });
    }
  }

  // ── Chat state ─────────────────────────────────────────────────────────────
  /** @type {ChatItem[]} */
  let items = $state([]);
  /** @type {import('$lib/ai.js').ApiMessage[]} */
  let apiHistory = $state([]);
  /** Full uncompressed history - never trimmed, always saved to IndexedDB */
  let rawApiHistory = $state([]);
  let loading = $state(false);
  let error = $state("");
  /** Provider payload behind the error banner - collapsed until asked for. */
  let errorDetailOpen = $state(false);

  /** @param {string} detail */
  async function copyErrorDetail(detail) {
    try {
      await navigator.clipboard.writeText(detail);
      toast.success("Error details copied");
    } catch {
      toast.error("Could not copy");
    }
  }
  /** Shown on the thinking row while waiting on rate-limit retries */
  let aiStatusHint = $state("");

  // ── Thinking phrase cycling ───────────────────────────────────────────────
  // Each phase pairs a label with an icon so the indicator communicates *what*
  // the agent is doing, not just that it's busy.
  const THINKING_PHRASES = [
    { text: "Thinking…", icon: Brain },
    { text: "Analyzing schema…", icon: Layers },
    { text: "Writing the query…", icon: PenLine },
    { text: "Reading the data…", icon: Database },
    { text: "Checking relationships…", icon: GitBranch },
    { text: "Running the numbers…", icon: BarChart2 },
    { text: "Exploring tables…", icon: Search },
    { text: "Crafting response…", icon: Sparkles },
    { text: "Connecting the dots…", icon: Zap },
    { text: "Almost there…", icon: Sparkles },
  ];
  let thinkingPhrase = $state(THINKING_PHRASES[0]);
  let thinkingVisible = $state(true);

  /** Pick an icon for an explicit status hint (e.g. "Analyzing schema…", "Rate limited…"). */
  function statusHintIcon(/** @type {string} */ hint) {
    if (/retry|rate.?limit/i.test(hint)) return RotateCcw;
    if (/schema|analyz/i.test(hint)) return Layers;
    if (/quer|writing/i.test(hint)) return PenLine;
    if (/read|fetch|load/i.test(hint)) return Database;
    if (/review|result|interpret|summar/i.test(hint)) return Search;
    if (/condens|compress|context/i.test(hint)) return Layers;
    return Sparkles;
  }
  // Single source of truth for what the loading indicator shows right now.
  const loadingText = $derived(aiStatusHint || thinkingPhrase.text);
  const loadingIcon = $derived(aiStatusHint ? statusHintIcon(aiStatusHint) : thinkingPhrase.icon);

  $effect(() => {
    if (!loading) {
      thinkingPhrase = THINKING_PHRASES[0];
      thinkingVisible = true;
      return;
    }
    let i = 0;
    let fadeId = 0;
    const tick = () => {
      thinkingVisible = false;
      fadeId = setTimeout(() => {
        fadeId = 0;
        i = (i + 1) % THINKING_PHRASES.length;
        thinkingPhrase = THINKING_PHRASES[i];
        thinkingVisible = true;
      }, 220);
    };
    const id = setInterval(tick, 2600);
    return () => { clearInterval(id); clearTimeout(fadeId); };
  });
  let inputText = $state("");
  /** Composer focus, so the shortcut hint only shows when it is useful. */
  let inputFocused = $state(false);
  /** The model in play - its brand mark identifies the chat surfaces. */
  const activeAiProfile = $derived(
    $aiProfiles.find((p) => p.id === $activeProfileId) ?? $aiProfiles[0],
  );
  const isDraftChat = $derived(!activeConvId && items.length > 0);
  /** Tracks all (name:args) combos executed this turn - prevents exact duplicate calls */
  let executedCalls = new Set();
  /**
   * Tracks failure count + last error per callKey this turn.
   * @type {Map<string, { count: number, lastError: string }>}
   */
  let failureTracker = new Map();

  // ── Streaming & abort ──────────────────────────────────────────────────────
  /** Accumulates text for the currently-streaming assistant turn */
  let streamingContent = $state("");
  /** Buffered full content waiting to be committed to state */
  let _pendingStreamContent = "";
  /** Pending commit timer (null = no pending update) */
  let _streamTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null);
  let _lastStreamCommit = 0;
  /** Min ms between streaming commits - caps marked.parse churn so long answers (tables/diagrams) stay smooth. */
  const STREAM_COMMIT_MS = 90;

  /**
   * Throttle streaming content updates. Re-parsing the full growing markdown on
   * every token is O(n²) and janks the UI; committing at most every ~90ms keeps
   * generation smooth even for large tables and diagrams.
   */
  function scheduleStreamingUpdate(content) {
    _pendingStreamContent = content;
    if (_streamTimer !== null) return;
    const elapsed = performance.now() - _lastStreamCommit;
    const delay = elapsed >= STREAM_COMMIT_MS ? 0 : STREAM_COMMIT_MS - elapsed;
    _streamTimer = setTimeout(() => {
      _streamTimer = null;
      _lastStreamCommit = performance.now();
      streamingContent = _pendingStreamContent;
    }, delay);
  }

  /** Flush any buffered streaming content immediately (called by stop() and finally block). */
  function flushStreamingContent() {
    if (_streamTimer !== null) {
      clearTimeout(_streamTimer);
      _streamTimer = null;
      _lastStreamCommit = performance.now();
      streamingContent = _pendingStreamContent;
    }
  }

  /** The streamed reply without its <plan> block, which draws as a checklist. */
  const streamingParts = $derived(
    streamingContent.includes("<plan>") ? extractPlan(streamingContent) : { plan: null, text: streamingContent },
  );
  /** Streaming display strips <think>...</think> blocks in real time */
  const displayStreamingContent = $derived(
    // Fast path: most models never emit <think>, so skip the regex scans over
    // the (growing) streamed string entirely unless a think tag is present.
    streamingParts.text.includes("<think>")
      ? streamingParts.text
          // Complete think blocks - hide entirely
          .replace(/<think>[\s\S]*?<\/think>/g, "")
          // Partial/open think block currently being written - hide from cursor onwards
          .replace(/<think>[\s\S]*$/, "")
          .trim()
      : streamingParts.text.trim(),
  );
  /** ID of the `streaming` ChatItem in `items` (null when not streaming) */
  let streamingId = $state(/** @type {string | null} */ (null));
  /** AbortController for the in-flight fetch; replaced each send() call */
  let abortController = /** @type {AbortController | null} */ (null);
  /**
   * Which turn owns the transcript's live state. Stop and every new turn bump
   * it; a turn whose number is no longer current leaves the UI alone. Without
   * it a stopped turn still settling (a slow query, a stream the provider kept
   * sending) went on writing into the next one and, finishing, cleared its
   * controller and spinner: the turn after that could not be stopped.
   */
  let turnSeq = 0;

  /** rAF handle for scroll debouncing during streaming */
  let rafId = /** @type {number | null} */ (null);
  /** True when user has manually scrolled away from bottom during streaming */
  let userScrolledUp = $state(false);

  /**
   * Items that draw nothing: a query card the Agent setting hides, or an
   * assistant turn with no text (it only called tools, or only thought). Each
   * still left its wrapper behind, and content-visibility gives an empty
   * wrapper a 120px placeholder, so nine hidden query cards were a thousand
   * pixels of blank transcript with a lone timestamp row in the middle.
   * @param {ChatItem} item
   */
  function drawsNothing(item) {
    if (item.kind === "result") return !$appAgentQueryCards && !item.error;
    if (item.kind === "assistant") return !item.parts?.some((p) => p.type !== "text" || p.content.trim());
    return false;
  }

  /** Sentinel pinned to the end of the transcript; see the observer below. */
  let bottomSentinel = $state(/** @type {HTMLElement | null} */ (null))

  // "Am I at the bottom?" used to be measured in the scroll handler, which read
  // scrollHeight/scrollTop/clientHeight on every scroll frame. Each of those
  // reads forces a synchronous layout - and with content-visibility:auto on the
  // messages, that layout has to resolve the off-screen subtrees it was meant to
  // skip. An IntersectionObserver answers the same question from the compositor,
  // costing nothing per frame.
  $effect(() => {
    const root = scrollEl
    const target = bottomSentinel
    if (!root || !target || typeof IntersectionObserver !== 'function') return
    const io = new IntersectionObserver(
      (entries) => { userScrolledUp = !entries[entries.length - 1].isIntersecting },
      { root, rootMargin: '0px 0px 80px 0px', threshold: 0 },
    )
    io.observe(target)
    return () => io.disconnect()
  })

  /** Scroll to bottom on the next animation frame (throttled; skipped if user scrolled up). */
  function scrollBottomSoon() {
    if (userScrolledUp || anchorId) return;
    if (rafId !== null) return;
    rafId = requestAnimationFrame(() => {
      rafId = null;
      pinToBottom();
    });
  }

  /**
   * Scroll the transcript to its end.
   *
   * Uses the sentinel rather than `scrollTop = scrollHeight`: reading scrollHeight
   * forces a synchronous layout of the whole transcript, and during streaming that
   * happened on every flushed chunk. scrollIntoView asks the engine to do the same
   * job without the app measuring anything.
   */
  function pinToBottom() {
    if (bottomSentinel) bottomSentinel.scrollIntoView({ block: 'end' });
    else if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight;
  }

  function jumpToBottom() {
    userScrolledUp = false;
    pinToBottom();
  }

  // ── Anchored turn ─────────────────────────────────────────────────────────
  // On send, the user's message moves to the top of the view and the answer
  // grows beneath it, as in T3 and ChatGPT, instead of the view chasing the
  // bottom on every token. A spacer under the turn gives it room until the
  // answer is tall enough to fill the view on its own.
  /** The user message the latest turn is pinned under. */
  let anchorId = $state(/** @type {string | null} */ (null));
  /** Room under the anchored turn, in px. */
  let tailSpace = $state(0);

  /**
   * Size the spacer from measurements, not guesses: just enough for the
   * message to scroll up to its stop (its scroll-margin under the floating
   * buttons). A guessed constant overshot, so a one-turn chat that fit on
   * screen still got a scrollbar.
   */
  function measureTail() {
    if (!anchorId || !scrollEl || !msgListEl) return;
    const anchor = msgListEl.querySelector(`[data-item-id="${anchorId}"]`);
    if (!(anchor instanceof HTMLElement)) return;
    const box = scrollEl.getBoundingClientRect();
    const anchorTop = anchor.getBoundingClientRect().top - box.top + scrollEl.scrollTop;
    const margin = parseFloat(getComputedStyle(anchor).scrollMarginTop) || 0;
    const needed = anchorTop - margin + scrollEl.clientHeight;
    const withoutSpacer = scrollEl.scrollHeight - tailSpace;
    const next = Math.max(0, Math.floor(needed - withoutSpacer));
    if (next !== tailSpace) tailSpace = next;
  }

  $effect(() => {
    if (!anchorId || !msgListEl || typeof ResizeObserver !== "function") return;
    const ro = new ResizeObserver(() => measureTail());
    ro.observe(msgListEl);
    return () => ro.disconnect();
  });

  /** @param {string} id */
  async function anchorTurn(id) {
    anchorId = id;
    userScrolledUp = false;
    await tick();
    measureTail();
    await tick();
    const anchor = msgListEl?.querySelector(`[data-item-id="${id}"]`);
    const still = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
    anchor?.scrollIntoView({ block: "start", behavior: still ? "auto" : "smooth" });
  }

  function clearAnchor() {
    anchorId = null;
    tailSpace = 0;
  }

  /**
   * End the running turn on screen, now, whatever it is waiting on.
   *
   * This used to return unless a live, un-aborted controller was in place, so
   * any path that left the spinner up without one (a run from a code block, a
   * turn cleared by an older one finishing) had a Stop button and an Escape
   * that did nothing. The UI is finalised here and the turn is disowned; the
   * work still in flight drops its results when it lands.
   */
  function stop() {
    if (!loading && !abortController) return;
    interruptAgentTurn();
    turnSeq++;
    const ctrl = abortController;
    abortController = null;
    try {
      ctrl?.abort();
      // A pending confirm would hold the tool loop forever: answer it as declined.
      for (const i of items.filter((i) => i.kind === "confirm")) i.resolve(false);
      settleOpenWork();
      flushStreamingContent();
      const partial = streamingContent.trim();
      const sid = streamingId;
      items = items
        .filter((i) => i.kind !== "thinking" && i.kind !== "executing")
        .map((i) =>
          sid && i.id === sid
            ? /** @type {ChatItem} */ ({
                id: sid,
                kind: "assistant",
                parts: parseAssistantMessage(extractPlan(partial).text || "…"),
                ts: Date.now(),
              })
            : i,
        )
        .filter((i) => i.kind !== "streaming");
    } finally {
      if (_streamTimer !== null) {
        clearTimeout(_streamTimer);
        _streamTimer = null;
      }
      _pendingStreamContent = "";
      streamingContent = "";
      streamingId = null;
      aiStatusHint = "";
      loading = false;
    }
  }

  function abortCurrentRequest() {
    if (_streamTimer !== null) {
      clearTimeout(_streamTimer);
      _streamTimer = null;
    }
    // Answer pending destructive-confirm prompts as declined; otherwise their
    // promises never resolve and the awaiting tool loop is retained forever.
    // Snapshot first: resolve() splices the item out of `items`.
    for (const i of items.filter((i) => i.kind === "confirm")) {
      i.resolve(false);
    }
    interruptAgentTurn();
    turnSeq++;
    if (abortController) {
      abortController.abort();
      abortController = null;
    }
    loading = false;
    streamingId = null;
    streamingContent = "";
    _pendingStreamContent = "";
    // Remove in-flight ephemeral items so the UI doesn't show a frozen state
    items = items.filter(
      (i) =>
        i.kind !== "thinking" &&
        i.kind !== "executing" &&
        i.kind !== "streaming",
    );
  }

  // ── Mermaid ───────────────────────────────────────────────────────────────
  /** App :root defines --muted, --accent, --border which inherit into SVG and override
   *  beautiful-mermaid's color-mix fallbacks - ER attributes/lines become illegible. */
  /** @param {import('$lib/themes/registry.js').ThemeId} themeId */
  // The app theme's tokens, as the Mermaid views use (the id only keys the cache).
  function resolveMermaidTheme(/** @type {string} */ _themeId) {
    return liveMermaidTheme();
  }

  /** @param {SVGSVGElement} svg @param {ReturnType<typeof resolveMermaidTheme>} theme */
  function applyMermaidThemeVars(svg, theme) {
    svg.style.setProperty("--bg", theme.bg);
    svg.style.setProperty("--fg", theme.fg);
    // Every variable the renderer reads is set on the SVG: left unset, `--line`
    // and `--accent` resolve from the page, where the same names are the app's
    // chrome tokens (arrows in the accent fill). Mixed from fg/bg, as in MermaidViewer.
    const mix = (/** @type {number} */ pct) => `color-mix(in srgb, ${theme.fg} ${pct}%, ${theme.bg})`;
    svg.style.setProperty("--muted", theme.muted ?? mix(62));
    svg.style.setProperty("--line", theme.line ?? mix(45));
    svg.style.setProperty("--accent", theme.accent ?? mix(85));
    svg.style.setProperty("--border", theme.border ?? mix(22));
    svg.style.setProperty("--surface", mix(4));
  }

  /** @type {Map<string, string>} */
  const mermaidCache = new Map();
  const MERMAID_CACHE_MAX = 30;

  /** `usecaseDiagram` is not a real Mermaid type - auto-correct it. */
  /** Diagram type keywords Mermaid recognises on the first line. */
  const MERMAID_TYPES = /^(?:%%[^\n]*\n\s*)*(?:erDiagram|flowchart|graph|sequenceDiagram|classDiagram|stateDiagram|mindmap|gitGraph|timeline|journey|gantt|pie|quadrantChart|xychart-beta|sankey-beta|block-beta|requirementDiagram|C4Context|usecaseDiagram)\b/;
  function normalizeMermaidCode(code) {
    let c = code.trim().replace(/^usecaseDiagram\b/m, "flowchart TD");
    // Models often send an ER fragment without its `erDiagram` header (an
    // entity block, or a relationship line). Mermaid then can't tell what it
    // is and fails with "No diagram type detected"; the syntax itself is ER.
    if (!MERMAID_TYPES.test(c)) {
      const er = /^\s*[A-Za-z_][\w-]*\s*\{\s*$/m.test(c) || /[|}o]\|?[-.]{2}[|{o]/.test(c);
      const flow = /^\s*[A-Za-z_][\w]*\s*(?:\[.*\]|\(.*\))?\s*-->/m.test(c);
      if (er) c = `erDiagram\n${c}`;
      else if (flow) c = `flowchart TD\n${c}`;
    }
    return c;
  }

  /** Reactive store for async-rendered diagrams (from full mermaid.js). */
  /** @type {Record<string, string>} */
  /** Capped LRU object for async mermaid SVGs - prevents unbounded memory growth. */
  const ASYNC_DIAGRAMS_MAX = 20;
  let _asyncDiagrams = $state({});
  let _asyncDiagramKeys = /** @type {string[]} */ ([]);
  let _mermaidJsInit = false;
  /** @type {typeof import('mermaid').default | null} Lazily-loaded mermaid module. */
  let _mermaid = null;

  async function _ensureMermaidJs() {
    if (_mermaidJsInit) return _mermaid;
    _mermaid = (await import("mermaid")).default;
    _mermaid.initialize({ startOnLoad: false, securityLevel: "loose" });
    _mermaidJsInit = true;
    return _mermaid;
  }

  async function _renderWithMermaidJs(code, asyncKey) {
    if (_asyncDiagrams[asyncKey] !== undefined) return;
    _asyncDiagrams[asyncKey] = ""; // mark as pending
    _asyncDiagramKeys.push(asyncKey);
    if (_asyncDiagramKeys.length > ASYNC_DIAGRAMS_MAX) {
      const evicted = _asyncDiagramKeys.shift();
      if (evicted) {
        const { [evicted]: _, ...rest } = _asyncDiagrams;
        _asyncDiagrams = rest;
      }
    }
    try {
      const mermaid = await _ensureMermaidJs();
      const id = `mermaid-${Math.random().toString(36).slice(2)}`;
      const { svg } = await mermaid.render(id, code);
      _asyncDiagrams[asyncKey] = svg;
    } catch (e) {
      const msg = String(e).replace(/</g, "&lt;").replace(/>/g, "&gt;");
      _asyncDiagrams[asyncKey] =
        `<div class="flex flex-col gap-1.5 p-3 rounded border border-destructive/30 bg-destructive/5"><p class="text-ui-xs font-medium text-destructive">Diagram render failed</p><p class="font-mono text-ui-3xs text-muted-foreground whitespace-pre-wrap">${msg}</p></div>`;
    }
  }

  /**
   * Split a streaming reply at a diagram fence: the prose before it renders as
   * markdown, the diagram shows as a generating card until the reply ends,
   * and anything after a closed fence renders again. Null when there is none.
   * @param {string} text
   */
  function splitStreamingDiagram(text) {
    const re = /```([\w-]*)[ \t]*\n/g;
    let m;
    while ((m = re.exec(text))) {
      const lang = m[1].toLowerCase();
      const start = m.index + m[0].length;
      const close = text.indexOf("```", start);
      const body = close === -1 ? text.slice(start) : text.slice(start, close);
      const kind = lang === "chart" || lang === "echarts" ? "chart"
        : lang === "mermaid" || lang === "mmd" || MERMAID_TYPES.test(body.trimStart()) ? "diagram" : null;
      if (kind) {
        return { kind, before: text.slice(0, m.index), after: close === -1 ? "" : text.slice(close + 3) };
      }
      if (close === -1) break;
      re.lastIndex = close + 3;
    }
    return null;
  }

  /**
   * A finished reply as transcript items: text runs stay one assistant item,
   * each ```chart block becomes a real chart item where it was written - the
   * same card render_chart makes (save, zoom, export). Local agents chart this
   * way, having no render_chart tool.
   * @param {string} id @param {any[]} parts @returns {ChatItem[]}
   */
  function replyItems(id, parts) {
    if (!parts.some((p) => p.type === "chart")) return [/** @type {ChatItem} */ ({ id, kind: "assistant", parts, ts: Date.now() })];
    /** @type {ChatItem[]} */
    const out = [];
    /** @type {any[]} */
    let run = [];
    let n = 0;
    const flush = () => {
      if (!run.length) return;
      out.push(/** @type {ChatItem} */ ({ id: n ? `${id}:${n}` : id, kind: "assistant", parts: run, ts: Date.now() }));
      n++;
      run = [];
    };
    for (const p of parts) {
      if (p.type !== "chart") { run.push(p); continue; }
      flush();
      const spec = { ...p.spec, data: chartRows(p.spec.data, p.spec) };
      out.push(/** @type {ChatItem} */ ({ id: uid(), kind: "chart", spec, error: spec.data.length ? null : "The chart came without data rows." }));
    }
    flush();
    if (!out.some((i) => i.id === id)) out[0] = /** @type {ChatItem} */ ({ ...out[0], id });
    return out;
  }

  /** The diagram's type as a label, from its Mermaid header line. @param {string} code */
  function diagramKind(code) {
    const head = (code.trim().split("\n").find((l) => l.trim() && !l.trim().startsWith("%%")) ?? "").trim().split(/\s+/)[0];
    const map = /** @type {Record<string, string>} */ ({
      erDiagram: "ER diagram", flowchart: "Flowchart", graph: "Flowchart", sequenceDiagram: "Sequence diagram",
      classDiagram: "Class diagram", "classDiagram-v2": "Class diagram", stateDiagram: "State diagram",
      "stateDiagram-v2": "State diagram", mindmap: "Mind map", gitGraph: "Git graph", timeline: "Timeline",
      journey: "User journey", gantt: "Gantt chart", pie: "Pie chart", quadrantChart: "Quadrant chart",
      "xychart-beta": "XY chart", "sankey-beta": "Sankey diagram",
    });
    return map[head] ?? "Diagram";
  }

  /** Render mermaid code to SVG. Tries beautiful-mermaid (sync), falls back to full mermaid.js (async). */
  function processMermaidSvg(code) {
    const themeId = normalizeThemeId(document.documentElement.dataset.theme);
    const normalized = normalizeMermaidCode(code);
    const cacheKey = `${themeId}:${normalized}`;

    // Sync cache hit (beautiful-mermaid)
    if (mermaidCache.has(cacheKey))
      return /** @type {string} */ (mermaidCache.get(cacheKey));

    // Async cache (full mermaid.js)
    const asyncKey = `async:${cacheKey}`;
    if (_asyncDiagrams[asyncKey] !== undefined) {
      if (_asyncDiagrams[asyncKey] === "") {
        return `<div class="flex items-center gap-2 p-4 text-ui-xs text-muted-foreground"><span class="size-3 animate-spin rounded-full border-2 border-border border-t-muted-foreground inline-block"></span>Rendering diagram…</div>`;
      }
      return _asyncDiagrams[asyncKey];
    }

    // Try beautiful-mermaid (sync, fast, themed)
    try {
      const svg = renderMermaidSync(normalized, resolveMermaidTheme(themeId));
      if (mermaidCache.size >= MERMAID_CACHE_MAX) {
        mermaidCache.delete(
          /** @type {string} */ (mermaidCache.keys().next().value),
        );
      }
      mermaidCache.set(cacheKey, svg);
      return svg;
    } catch {
      // Fall through to full mermaid.js
    }

    // Trigger async render and show loading spinner
    void _renderWithMermaidJs(normalized, asyncKey);
    return `<div class="flex items-center gap-2 p-4 text-ui-xs text-muted-foreground"><span class="size-3 animate-spin rounded-full border-2 border-border border-t-muted-foreground inline-block"></span>Rendering diagram…</div>`;
  }

  /** Svelte action: applies live dark/light theming + pan/zoom to the mermaid canvas.
   *  Zoom: Ctrl/Meta + scroll (so regular chat scroll is never blocked).
   *  Pan: drag. Reset: double-click. */
  function mermaidInteractive(/** @type {HTMLDivElement} */ node) {
    // The SVG may not exist yet: a diagram the fast renderer can't draw goes to
    // mermaid.js asynchronously, and the card mounts on a "Rendering…" spinner.
    // Attaching only at mount left those diagrams unthemed with no pan or zoom,
    // so this waits for the SVG and re-attaches whenever it is replaced.
    /** @type {{ destroy(): void } | null} */
    let live = null;
    /** @type {Element | null} */
    let current = null;
    const sync = () => {
      const svg = /** @type {SVGSVGElement|null} */ (node.querySelector("svg"));
      if (svg === current) return;
      live?.destroy();
      live = null;
      current = svg;
      if (svg) live = attachMermaid(node, svg);
    };
    sync();
    const mo = new MutationObserver(sync);
    mo.observe(node, { childList: true, subtree: true });
    return {
      destroy() {
        mo.disconnect();
        live?.destroy();
      },
    };
  }

  /** Theming and pan/zoom for one rendered diagram SVG.
   *  @param {HTMLDivElement} node @param {SVGSVGElement} svg */
  function attachMermaid(node, svg) {

    // ── Theming ────────────────────────────────────────────────────────────
    function applyTheme() {
      const themeId = normalizeThemeId(document.documentElement.dataset.theme);
      const theme = resolveMermaidTheme(themeId);
      applyMermaidThemeVars(svg, theme);
      node.style.background = theme.bg;
    }
    applyTheme();
    const themeObs = new MutationObserver(applyTheme);
    themeObs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class", "data-theme"],
    });

    // ── Pan / zoom ─────────────────────────────────────────────────────────
    svg.style.transformOrigin = "0 0";
    let scale = 1,
      tx = 0,
      ty = 0;
    let dragging = false,
      ox = 0,
      oy = 0;

    function applyTransform(animate = false) {
      svg.style.transition = animate ? "transform 0.25s ease" : "";
      svg.style.transform = `translate(${tx}px,${ty}px) scale(${scale})`;
    }

    const onWheel = (/** @type {WheelEvent} */ e) => {
      if (!e.ctrlKey && !e.metaKey) return; // regular scroll → let it bubble for chat scroll
      e.preventDefault();
      const { left, top } = node.getBoundingClientRect();
      const mx = e.clientX - left,
        my = e.clientY - top;
      const factor = e.deltaY < 0 ? 1.12 : 0.89;
      const ns = Math.max(0.1, Math.min(10, scale * factor));
      tx = mx - (mx - tx) * (ns / scale);
      ty = my - (my - ty) * (ns / scale);
      scale = ns;
      applyTransform();
    };

    let rafId = 0;
    const onMove = (/** @type {MouseEvent} */ e) => {
      if (!dragging) return;
      tx = e.clientX - ox;
      ty = e.clientY - oy;
      if (rafId) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        rafId = 0;
        applyTransform();
      });
    };

    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      node.classList.remove("is-dragging");
      // Detach the global move/up listeners until the next drag begins, so we
      // don't run a window-wide mousemove handler whenever a diagram is shown.
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    const onDown = (/** @type {MouseEvent} */ e) => {
      if (e.button !== 0) return;
      dragging = true;
      ox = e.clientX - tx;
      oy = e.clientY - ty;
      node.classList.add("is-dragging");
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    };

    const onDblClick = () => {
      scale = 1;
      tx = 0;
      ty = 0;
      applyTransform(true);
    };

    const onZoomIn = () => {
      scale = Math.min(10, scale * 1.25);
      applyTransform();
    };
    const onZoomOut = () => {
      scale = Math.max(0.1, scale / 1.25);
      applyTransform();
    };
    const onZoomReset = () => {
      scale = 1;
      tx = 0;
      ty = 0;
      applyTransform(true);
    };

    node.addEventListener("wheel", onWheel, { passive: false });
    node.addEventListener("mousedown", onDown);
    node.addEventListener("dblclick", onDblClick);
    node.addEventListener("diagram:zoomin", onZoomIn);
    node.addEventListener("diagram:zoomout", onZoomOut);
    node.addEventListener("diagram:reset", onZoomReset);

    return {
      destroy() {
        themeObs.disconnect();
        if (rafId) cancelAnimationFrame(rafId);
        node.removeEventListener("wheel", onWheel);
        node.removeEventListener("mousedown", onDown);
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        node.removeEventListener("dblclick", onDblClick);
        node.removeEventListener("diagram:zoomin", onZoomIn);
        node.removeEventListener("diagram:zoomout", onZoomOut);
        node.removeEventListener("diagram:reset", onZoomReset);
      },
    };
  }

  // ── Chart fullscreen / zoom mode ───────────────────────────────────────────
  /** @type {{ spec: any, title: string } | null} */
  let fullscreenChart = $state(null);

  function openChartFullscreen(spec) {
    fullscreenChart = { spec, title: spec.title || "" };
  }

  function closeChartFullscreen() {
    fullscreenChart = null;
  }

  // ── Diagram fullscreen ─────────────────────────────────────────────────────
  /** @type {string | null} */
  let fullscreenDiagramCode = $state(null);
  /** @type {HTMLDivElement | null} */
  let fullscreenCanvasEl = $state(null);

  function openDiagramFullscreen(code) {
    fullscreenDiagramCode = code;
  }

  function closeDiagramFullscreen() {
    fullscreenDiagramCode = null;
  }

  function dispatchDiagramEvent(name) {
    fullscreenCanvasEl?.dispatchEvent(new CustomEvent(name));
  }

  async function exportDiagramSvg() {
    const canvas =
      fullscreenCanvasEl ?? document.querySelector(".mermaid-canvas");
    const svgEl =
      canvas?.querySelector("svg") ??
      document.querySelector(".mermaid-canvas svg");
    if (!svgEl) {
      toast.error("No diagram to export");
      return;
    }
    const serializer = new XMLSerializer();
    const svgStr =
      '<?xml version="1.0" encoding="UTF-8"?>\n' +
      serializer.serializeToString(svgEl);
    try {
      const path = await saveExportAs(svgStr, "diagram.svg", {
        name: "SVG image",
        extensions: ["svg"],
      });
      if (!path) return; // save dialog cancelled
      toast.success("SVG exported", { description: `Saved to ${path}` });
    } catch (e) {
      toast.error("Could not export the SVG", { description: String(e) });
    }
  }

  async function exportDiagramPng() {
    const canvas =
      fullscreenCanvasEl ?? document.querySelector(".mermaid-canvas");
    const svgEl =
      canvas?.querySelector("svg") ??
      document.querySelector(".mermaid-canvas svg");
    if (!svgEl) {
      toast.error("No diagram to export");
      return;
    }
    try {
      const blob = await svgToPngBlob(svgEl, { scale: 2, background: "#ffffff" });
      const path = await downloadBlob(blob, "diagram.png");
      if (path === null) return; // save dialog cancelled
      toast.success("PNG exported", {
        description: path ? `Saved to ${path}` : "diagram.png saved to your downloads",
      });
    } catch (e) {
      toast.error("Could not export PNG", {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  /** @type {HTMLDivElement | null} */
  let scrollEl = $state(null);
  /** @type {HTMLElement | null} The messages content wrapper (observed to stay pinned to bottom). */
  let msgListEl = $state(null);

  // Stick-to-bottom: the final assistant message renders asynchronously (markdown +
  // shiki highlighting), and charts/mermaid settle even later, so a one-shot scroll
  // fires too early. Instead, observe the message content and keep the view pinned to
  // the bottom whenever it grows - unless the user has deliberately scrolled up.
  $effect(() => {
    const content = msgListEl;
    if (!content) return;
    const ro = new ResizeObserver(() => {
      // An anchored turn keeps its place; the answer grows below it.
      if (!userScrolledUp && !anchorId) pinToBottom();
    });
    ro.observe(content);
    return () => ro.disconnect();
  });

  /** @type {HTMLTextAreaElement | null} */
  let inputRef = $state(null);

  // ── Input undo / redo ─────────────────────────────────────────────────────
  /** @type {string[]} */
  let inputHistory = [""];
  let inputHistoryIdx = 0;
  /** @type {ReturnType<typeof setTimeout> | null} */
  let historyTimer = null;

  const INPUT_HISTORY_MAX = 100;

  function pushHistory(text) {
    if (historyTimer) clearTimeout(historyTimer);
    historyTimer = setTimeout(() => {
      historyTimer = null;
      if (text === inputHistory[inputHistoryIdx]) return;
      let next = [...inputHistory.slice(0, inputHistoryIdx + 1), text];
      // Cap the input-undo stack so a long editing session can't grow it forever.
      if (next.length > INPUT_HISTORY_MAX) next = next.slice(next.length - INPUT_HISTORY_MAX);
      inputHistory = next;
      inputHistoryIdx = inputHistory.length - 1;
    }, 250);
  }

  function undoInput() {
    if (historyTimer) {
      clearTimeout(historyTimer);
      historyTimer = null;
    }
    if (inputText !== inputHistory[inputHistoryIdx]) {
      inputHistory = [...inputHistory.slice(0, inputHistoryIdx + 1), inputText];
      inputHistoryIdx = inputHistory.length - 1;
    }
    if (inputHistoryIdx <= 0) return;
    inputHistoryIdx--;
    inputText = inputHistory[inputHistoryIdx];
    void tick().then(() => {
      resizeInput();
      inputRef?.focus();
    });
  }

  function redoInput() {
    if (inputHistoryIdx >= inputHistory.length - 1) return;
    inputHistoryIdx++;
    inputText = inputHistory[inputHistoryIdx];
    void tick().then(() => {
      resizeInput();
      inputRef?.focus();
    });
  }

  function resetHistory() {
    if (historyTimer) {
      clearTimeout(historyTimer);
      historyTimer = null;
    }
    inputHistory = [""];
    inputHistoryIdx = 0;
  }

  const canUndo = $derived(
    inputHistoryIdx > 0 || inputText !== inputHistory[inputHistoryIdx],
  );
  const canRedo = $derived(inputHistoryIdx < inputHistory.length - 1);

  // ── Global keyboard shortcuts ─────────────────────────────────────────────
  onMount(() => {
    function onGlobal(/** @type {KeyboardEvent} */ e) {
      if (!isActive) return;

      // `/` → focus input
      if (e.key === "/") {
        const tag =
          /** @type {HTMLElement | null} */ (document.activeElement)?.tagName ??
          "";
        if (tag === "INPUT" || tag === "TEXTAREA") return;
        if (
          /** @type {HTMLElement | null} */ (document.activeElement)
            ?.isContentEditable
        )
          return;
        e.preventDefault();
        inputRef?.focus();
        return;
      }

      // ⌘⇧, → toggle settings panel
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === ",") {
        e.preventDefault();
        settingsOpen = !settingsOpen;
        if (settingsOpen) settingsTab = "model";
      }
    }
    document.addEventListener("keydown", onGlobal);
    onDestroy(() => document.removeEventListener("keydown", onGlobal));
  });

  let seq = 0;
  const uid = () => `c${++seq}`;
  /** After loading stored items, advance seq past their highest ID so new uid()s never collide. */
  function syncSeq(loadedItems) {
    for (const item of loadedItems) {
      const m = /^c(\d+)$/.exec(String(item?.id ?? ""));
      if (m) seq = Math.max(seq, parseInt(m[1]));
    }
  }

  const hasPendingConfirm = $derived(items.some((i) => i.kind === "confirm"));
  const suggestions = $derived(generateSuggestions(schemaContext));
  /** The database the chat is about, for the landing headline. */
  const dbName = $derived(String(/** @type {any} */ (schemaContext).database ?? ""));
  // The composer moves between the landing (centred) and the foot of the
  // transcript, which remounts it; keep focus in it across that move.
  /** @type {boolean | null} */
  let composerWasEmpty = null;
  $effect(() => {
    const empty = items.length === 0;
    const first = composerWasEmpty === null;
    if (empty === composerWasEmpty) return;
    composerWasEmpty = empty;
    if (first) return;
    const hadFocus = inputFocused || !document.activeElement || document.activeElement === document.body;
    if (hadFocus) void tick().then(() => inputRef?.focus());
  });
  /** Show a persistent activity row while loading when no thinking/streaming/executing row is already visible. */
  const showWorking = $derived(
    loading &&
      !items.some(
        (i) =>
          i.kind === "thinking" ||
          i.kind === "streaming" ||
          i.kind === "executing" ||
          // A local agent's fold already says it is working.
          (i.kind === "work" && i.endedAt === null),
      ),
  );

  /** System prompt for the current AI turn - built fresh each send() with selective schema injection */
  let turnSystemPrompt = $state("");

  /**
   * Session-level schema cache: key = "schema.table", value = column list.
   * Accumulates across all turns so we only fetch each table once per session.
   * Gets merged into allTableColumns when building the system prompt.
   * @type {Record<string, {name:string, dataType:string, nullable?:boolean}[]>}
   */
  let fetchedSchemas = $state({});
  /**
   * A few real rows per table, keyed `schema.table`, injected into the system
   * prompt alongside that table's columns.
   * @type {Record<string, { rows: Record<string, unknown>[], truncated: boolean }>}
   */
  let sampledRows = $state({});

  // ── Context window stats ──────────────────────────────────────────────────
  /** Rough token estimate: 1 token ≈ 4 chars (GPT/Mistral rule of thumb) */
  function tokEst(chars) {
    const t = Math.round(chars / 4);
    if (t >= 10_000) return `~${(t / 1000).toFixed(0)}k`;
    if (t >= 1_000) return `~${(t / 1000).toFixed(1)}k`;
    return `~${t}`;
  }

  const contextStats = $derived.by(() => {
    const historyChars = apiHistory.reduce(
      (s, m) =>
        s +
        (typeof m.content === "string"
          ? m.content.length
          : JSON.stringify(m.content ?? "").length),
      0,
    );
    const promptChars = turnSystemPrompt.length || 0;
    // The tool schemas ride on every request too. Left out, a "hi" read ~5.7k
    // while costing ~8k.
    const toolsChars = JSON.stringify(turnTools).length;
    const totalChars = historyChars + promptChars + toolsChars;
    const maxChars = 60_000;
    const historyTokens = Math.round(historyChars / 4);
    const promptTokens = Math.round(promptChars / 4);
    const totalTokens = Math.round(totalChars / 4);
    const maxTokens = Math.round(maxChars / 4);
    const pct = Math.min(100, Math.round((totalTokens / maxTokens) * 100));
    return {
      historyChars,
      promptChars,
      toolsChars,
      totalChars,
      historyTokens,
      promptTokens,
      totalTokens,
      maxChars,
      maxTokens,
      pct,
      messages: apiHistory.length,
    };
  });

  // ── AI sidebar visibility (persisted) ─────────────────────────────────────
  const AI_SIDEBAR_KEY = "stroke:ai-sidebar-open";

  // ── Chat list width ───────────────────────────────────────────────────────
  const AI_SIDEBAR_W_KEY = "stroke:ai-sidebar-width";
  const SIDEBAR_MIN_W = 160;
  const SIDEBAR_MAX_W = 420;
  const SIDEBAR_DEFAULT_W = 208;

  function loadSidebarWidth() {
    // The full-window chat list is the window's left column, like the table
    // sidebar: one shared width (layout.js), so switching modes never resizes it.
    if (externalToggle) return loadLayout().navSidebarWidth;
    try {
      const w = Number(localStorage.getItem(AI_SIDEBAR_W_KEY) || SIDEBAR_DEFAULT_W);
      return Math.min(SIDEBAR_MAX_W, Math.max(SIDEBAR_MIN_W, w || SIDEBAR_DEFAULT_W));
    } catch {
      return SIDEBAR_DEFAULT_W;
    }
  }

  let sidebarWidth = $state(loadSidebarWidth());
  // Follow a resize of the table sidebar (full-window mode shares its width).
  $effect(() => {
    if (!externalToggle) return;
    const on = (/** @type {Event} */ e) => {
      const w = /** @type {CustomEvent<number>} */ (e).detail;
      if (Number.isFinite(w)) sidebarWidth = w;
    };
    window.addEventListener(NAV_WIDTH_EVENT, on);
    return () => window.removeEventListener(NAV_WIDTH_EVENT, on);
  });
  let sidebarResizeStart = 0;
  let sidebarResizeScale = 1;

  function saveSidebarWidth() {
    if (externalToggle) {
      sidebarWidth = setNavSidebarWidth(sidebarWidth);
      return;
    }
    try {
      localStorage.setItem(AI_SIDEBAR_W_KEY, String(Math.round(sidebarWidth)));
    } catch {
      /* ignore */
    }
  }

  function loadSidebarPref() {
    try {
      return localStorage.getItem(AI_SIDEBAR_KEY) !== "false";
    } catch {
      return true;
    }
  }

  function saveSidebarPref(v) {
    try {
      localStorage.setItem(AI_SIDEBAR_KEY, String(v));
    } catch {}
  }

  let sidebarVisible = $state(loadSidebarPref());

  /** Send a message from outside (Fix with AI, palette quick-ask). @param {string} text */
  export function sendMessage(text) {
    void send([text]);
  }

  /** Put text in the composer for the user to finish. @param {string} text */
  export function draftMessage(text) {
    inputText = text + inputText;
  }

  /** For the title bar's sidebar button in AI mode. */
  export function toggleChats() {
    toggleAiSidebar()
  }

  function toggleAiSidebar() {
    sidebarVisible = !sidebarVisible;
    saveSidebarPref(sidebarVisible);
  }

  /** Accordion: ID of the currently expanded result card (null = all collapsed) */
  let openResultId = $state(/** @type {string | null} */ (null));
  /** Result card whose raw (un-humanized) driver error is expanded. */
  let rawErrorId = $state(/** @type {string | null} */ (null));
  /** Whether the user has manually collapsed results - respected by auto-open logic */
  let userPrefersCollapsed = $state(false);

  /** @param {string} id */
  function toggleResult(id) {
    if (openResultId === id) {
      openResultId = null;
      userPrefersCollapsed = true;
    } else {
      openResultId = id;
      userPrefersCollapsed = false;
    }
  }

  /** Open a result card. Schema results (isSchema=true) are always kept collapsed. */
  function autoOpenResult(/** @type {string} */ id, isSchema = false) {
    if (!userPrefersCollapsed && !isSchema) openResultId = id;
  }

  /** IDs of chart items the user has already saved this session */
  let savedChartIds = $state(/** @type {Set<string>} */ (new Set()));
  /** IDs of diagram items the user has explicitly saved this session */
  let savedDiagramIds = $state(/** @type {Set<string>} */ (new Set()));

  /** Blocks collapsed by user (in set = collapsed; default open for both code and SQL) */
  let collapsed = $state(/** @type {Set<string>} */ (new Set()));

  /** @param {string} key */
  function toggleCollapse(key) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }

  // ── Input auto-resize ──────────────────────────────────────────────────────
  function resizeInput() {
    const el = inputRef;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = Math.max(60, Math.min(el.scrollHeight, 200)) + "px";
  }

  function resetInputHeight() {
    if (inputRef) inputRef.style.height = "auto";
  }

  async function scrollBottom() {
    await tick();
    // An anchored turn stays where it is; the answer grows into the space below.
    if (anchorId) return;
    userScrolledUp = false;
    pinToBottom();
  }

  /** @param {KeyboardEvent} e */
  function handleKeydown(/** @type {KeyboardEvent} */ e) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
      return;
    }
    // Ctrl/Cmd + Z → undo. The key is lower-cased: with Shift held it arrives
    // as "Z", so the redo chord below never matched.
    const k = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && !e.altKey && k === "z" && !e.shiftKey) {
      e.preventDefault();
      undoInput();
      return;
    }
    // Ctrl/Cmd + Shift + Z  or  Ctrl + Y → redo
    if (
      ((e.ctrlKey || e.metaKey) && !e.altKey && k === "z" && e.shiftKey) ||
      (e.ctrlKey && !e.altKey && k === "y")
    ) {
      e.preventDefault();
      redoInput();
      return;
    }
    // Ctrl/Cmd + Backspace is left to the field: a word on Windows and Linux,
    // the line on macOS, as in every other box in the app. It used to clear the
    // whole message, which on Windows is where you reach to delete one word.
  }

  /**
   * Fetch column definitions for ALL tables in the active schema that aren't yet cached.
   * After the first call this is a no-op (all tables cached). Stores in fetchedSchemas,
   * NOT in apiHistory - gets merged into the system prompt each turn.
   */
  async function ensureFullSchemaCache() {
    if (!schemaContext.tables?.length) return;
    // Schemaless engines: fields come from sampled documents, not a catalog.
    if (queryLanguageFor(schemaContext.dbType)) return;
    const dbTypeSC = schemaContext.dbType ?? "postgres";
    const isMysql = dbTypeSC === "mysql";
    const isSqliteFamilySC =
      dbTypeSC === "sqlite" || dbTypeSC === "d1" || dbTypeSC === "libsql";
    const sc = schemaContext.activeSchema;

    const combined = { ...schemaContext.allTableColumns, ...fetchedSchemas };
    const missing = schemaContext.tables.filter(
      (t) => !combined[`${sc}.${t.name}`],
    );
    if (!missing.length) return;

    try {
      /** @type {Record<string, {name:string, dataType:string, nullable:boolean}[]>} */
      const byTable = {};

      if (isSqliteFamilySC) {
        // SQLite/D1/LibSQL: PRAGMA per table (no information_schema).
        // In parallel - N serial IPC round-trips would stall the first message.
        await Promise.all(
          missing.map(async (t) => {
            try {
              const tq = `"${t.name.replace(/"/g, '""')}"`;
              const data = await executeSql(`PRAGMA table_info(${tq})`);
              const c = data.columns ?? [],
                r = data.rows ?? [];
              const nameI = c.findIndex((x) => x.name === "name"),
                typeI = c.findIndex((x) => x.name === "type");
              const nnI = c.findIndex((x) => x.name === "notnull");
              const key = `${sc}.${t.name}`;
              byTable[key] = r.map((row) => ({
                name: String(row[nameI] ?? row[1] ?? ""),
                dataType: String(row[typeI] ?? row[2] ?? "text"),
                nullable: !(row[nnI] ?? row[3]),
              }));
            } catch {
              /* skip this table */
            }
          }),
        );
      } else {
        const scSafe = sc.replace(/'/g, "''");
        const sql = isMysql
          ? `SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${scSafe}' ORDER BY TABLE_NAME, ORDINAL_POSITION`
          : `SELECT c.table_name, c.column_name, CASE WHEN c.data_type = 'USER-DEFINED' THEN c.udt_name ELSE c.data_type END AS data_type, c.is_nullable FROM information_schema.columns c WHERE c.table_schema = '${scSafe}' ORDER BY c.table_name, c.ordinal_position`;
        const data = await executeSql(sql);
        for (const row of data.rows ?? []) {
          const key = `${sc}.${String(row[0])}`;
          if (!byTable[key]) byTable[key] = [];
          byTable[key].push({
            name: String(row[1]),
            dataType: String(row[2]),
            nullable: String(row[3]).toUpperCase() === "YES",
          });
        }
      }

      fetchedSchemas = { ...fetchedSchemas, ...byTable };
    } catch {
      // Non-fatal - AI will call describe_table if schema fetch fails
    }
  }

  /** Rows sampled per table. Two or three is enough to show the shape of the data. */
  const SAMPLE_ROWS = 3;
  /** Tables sampled per turn. A wide schema would otherwise cost one query each. */
  const SAMPLE_TABLE_CAP = 6;
  /** Longest sampled value kept intact; past this the prompt cost stops paying off. */
  const SAMPLE_VALUE_CHARS = 120;

  /**
   * Read a few real rows from each table about to be described in the prompt.
   *
   * Types say a column is `text`; they don't say it holds 'active' rather than
   * 'ACTIVE' or '1', that a date is stored as an epoch string, or that a
   * nullable column is null for every row that matters. Queries written against
   * the type alone get those wrong, so the model sees actual values before it
   * writes any SQL. Cached per table for the session - the shape of a table's
   * data doesn't change between turns.
   *
   * Best effort throughout: a table that can't be read is simply left out.
   * @param {string[]} keys `schema.table` keys to sample
   */
  async function ensureSampleRows(keys) {
    const pending = keys.filter((k) => !sampledRows[k]).slice(0, SAMPLE_TABLE_CAP);
    if (!pending.length) return;

    /** @type {Record<string, { rows: Record<string, unknown>[], truncated: boolean }>} */
    const found = {};
    // In parallel: each is one round trip to the database, and on a far one
    // six in a row was most of the wait before the first token.
    await Promise.all(pending.map(async (key) => {
      const schema = key.slice(0, key.indexOf("."));
      const table = key.slice(key.indexOf(".") + 1);
      // NoSQL: the engine's own read (db.users.find().limit(3)); none where
      // any read would scan (DynamoDB, Redis).
      const nosql = queryLanguageFor(schemaContext.dbType);
      const nosqlQuery = nosql ? sampleQuery(String(schemaContext.dbType), schema, table, SAMPLE_ROWS) : null;
      if (nosql && !nosqlQuery) return;
      try {
        const data = await executeSql(
          nosqlQuery ?? `SELECT * FROM ${tableRefForEngine(schema, table)} LIMIT ${SAMPLE_ROWS}`,
        );
        const cols = (data.columns ?? []).map((c) => c.name);
        let truncated = false;
        const rows = (data.rows ?? []).map((r) =>
          Object.fromEntries(
            cols.map((n, i) => {
              const v = /** @type {any[]} */ (r)[i];
              if (typeof v === "string" && v.length > SAMPLE_VALUE_CHARS) {
                truncated = true;
                return [n, v.slice(0, SAMPLE_VALUE_CHARS) + "…"];
              }
              return [n, v];
            }),
          ),
        );
        // Cache the empty result too, so an empty table isn't re-queried every turn.
        found[key] = { rows, truncated };
      } catch {
        /* unreadable table - skip it, the model can still use the columns */
      }
    }));
    if (Object.keys(found).length) sampledRows = { ...sampledRows, ...found };
  }

  /**
   * Quoted, schema-qualified table reference for the connected engine.
   * SQLite/D1/LibSQL have no user schemas, so the schema half is dropped there -
   * qualifying would turn a valid name into a missing-table error.
   * @param {string} schema @param {string} table
   */
  function tableRefForEngine(schema, table) {
    const db = schemaContext.dbType ?? "postgres";
    if (db === "sqlite" || db === "d1" || db === "libsql") {
      return `"${table.replace(/"/g, '""')}"`;
    }
    if (db === "mysql") {
      const q = (/** @type {string} */ s) => `\`${s.replace(/`/g, "``")}\``;
      return schema ? `${q(schema)}.${q(table)}` : q(table);
    }
    const q = (/** @type {string} */ s) => `"${s.replace(/"/g, '""')}"`;
    return schema ? `${q(schema)}.${q(table)}` : q(table);
  }

  /**
   * Messages sent while a turn runs. They wait above the composer and go out
   * one at a time as turns end, instead of Enter doing nothing mid-turn.
   * Stop leaves them waiting; switching chats drops them.
   */
  let queued = $state(/** @type {{ id: string, text: string }[]} */ ([]));

  /** Send the next queued message, once nothing is running. */
  function drainQueue() {
    if (loading || hasPendingConfirm || queued.length === 0) return;
    const [next, ...rest] = queued;
    queued = rest;
    void send([next.text]);
  }

  /** @param {string} id */
  function unqueue(id) {
    queued = queued.filter((q) => q.id !== id);
  }

  async function send(/** @type {string} */ [overrideText] = []) {
    const text = (overrideText ?? inputText).trim();
    if (!text || hasPendingConfirm) return;
    if (loading) {
      // Typed while the agent works: hold it for when the turn ends.
      if (overrideText === undefined) {
        queued = [...queued, { id: uid(), text }];
        inputText = "";
        resetInputHeight();
        resetHistory();
      }
      return;
    }
    error = "";
    aiStatusHint = "";
    if (!overrideText) {
      inputText = "";
      resetInputHeight();
      resetHistory();
    }

    const userItemId = uid();
    items.push(
      /** @type {ChatItem} */ ({ id: userItemId, kind: "user", text, ts: Date.now() }),
    );
    apiHistory.push({ role: "user", content: text });
    rawApiHistory.push({ role: "user", content: text });
    await anchorTurn(userItemId);

    const thinkingId = uid();
    items.push(/** @type {ChatItem} */ ({ id: thinkingId, kind: "thinking" }));
    await scrollBottom();

    loading = true;
    const turn = ++turnSeq;
    const ctrl = new AbortController();
    abortController = ctrl;
    /** Stopped, or replaced by a newer turn: leave the shared state alone. */
    const disowned = () => turn !== turnSeq;
    executedCalls = new Set();
    failureTracker = new Map();
    builtinWorkId = null;
    pendingPlan = null;

    // A local agent CLI reads the schema through its own tools, so none of the
    // prompt building below applies to it.
    const agent = profileAgent($aiSettings);
    if (agent) {
      try {
        const reply = await runAgentTurn(agent, $aiSettings.model ?? "", text, disowned);
        if (disowned()) return;
        if (reply) {
          apiHistory.push({ role: "assistant", content: reply });
          rawApiHistory.push({ role: "assistant", content: reply });
        }
        await persistCurrent();
        void generateAiTitle();
      } catch (e) {
        if (!disowned()) error = String(/** @type {any} */ (e)?.message ?? e);
      } finally {
        if (!disowned()) await finishTurn();
      }
      return;
    }

    // Proactively fetch column definitions for all tables into session cache
    // only when the query looks like it's asking about data (not a trivial message).
    const looksLikeDataQuery =
      text.length > 4 ||
      /select|from|show|list|count|table|schema|column|insert|update|delete/i.test(
        text,
      );
    if (looksLikeDataQuery) {
      aiStatusHint = "Analyzing schema…";
      await ensureFullSchemaCache();
      if (disowned()) return;
      aiStatusHint = "";
    }

    // Build query-filtered system prompt for this turn (merge session cache)
    const filteredCtx = filterSchemaForQuery(
      {
        ...schemaContext,
        allTableColumns: {
          ...schemaContext.allTableColumns,
          ...fetchedSchemas,
        },
        userSkills: skills,
      },
      text,
    );

    // Sample the tables this turn is actually about, before any SQL is written.
    // filterSchemaForQuery has already narrowed to the active table plus the ones
    // named in the question, so this samples what the model is about to reason
    // over rather than the whole schema.
    if (looksLikeDataQuery) {
      const keys = Object.keys(filteredCtx.allTableColumns ?? {});
      if (keys.length) {
        aiStatusHint = "Reading sample rows…";
        await ensureSampleRows(keys);
        if (disowned()) return;
        aiStatusHint = "";
      }
    }

    // What this turn needs beyond the core harness. The chart and diagram
    // skills and tools ride along only once the conversation has asked for
    // them; a question about row counts used to carry ~3.5k tokens of them.
    const topics = detectPromptTopics(
      text,
      detectPromptTopics(
        rawApiHistory
          .filter((m) => m.role === "user")
          .map((m) => String(m.content ?? ""))
          .join("\n"),
      ),
    );
    turnTools = toolsForTurn(topics, $appAgentWebAccess);
    const basePrompt = buildSystemPrompt({
      ...filteredCtx,
      sampleRows: sampledRows,
      webAccess: $appAgentWebAccess,
      topics,
      // So "which model are you?" gets the real answer, not a guess.
      modelLabel: modelDisplayName(settings),
      // The transcript draws <plan> as a ticking checklist (ai-steps.js).
      planSteps: true,
    });
    const ci = $aiChatParams.customInstructions.trim();
    turnSystemPrompt = ci ? `${ci}\n\n---\n\n${basePrompt}` : basePrompt;

    // Smart context management: sliding window + optional summarization.
    // managedLen marks where new messages start after the turn - used to append to rawApiHistory.
    // How much the request carries depends on the endpoint (historyBudget): the
    // free gateway gets a smaller window and no summarising call.
    const { history: managedHistory, summarized } = await manageHistory(
      settings,
      apiHistory,
      {
        ...historyBudget(settings),
        onStatus: (msg) => {
          if (!disowned()) aiStatusHint = msg;
        },
      },
    );
    if (disowned()) return;
    const managedLen = managedHistory.length;
    if (summarized) {
      apiHistory = managedHistory;
      aiStatusHint = "";
    } else {
      apiHistory = managedHistory;
    }

    try {
      await runAiTurn(0, ctrl);
      if (disowned()) return;
      // Append all messages added during this turn to the full uncompressed history
      rawApiHistory.push(...apiHistory.slice(managedLen));
      await persistCurrent();
      // Generate AI title after the first turn, in the background
      void generateAiTitle();
    } catch (e) {
      if (/** @type {any} */ (e)?.name !== "AbortError" && !disowned()) error = String(e);
    } finally {
      // Stop already finalised the screen, and a newer turn may own it now.
      if (!disowned()) await finishTurn();
    }
  }

  /** Close out the current turn's UI: the streamed reply kept, indicators gone. */
  async function finishTurn() {
    try {
      settleOpenWork();
      // Flush any rAF-buffered content before reading it
      flushStreamingContent();
      // Finalize any in-progress streaming item (abort or error mid-stream)
      if (streamingId) {
        const partial = streamingContent.trim();
        const sid = streamingId;
        items = items
          .filter((i) => i.kind !== "thinking" && i.kind !== "executing")
          .map((i) =>
            i.id === sid
              ? /** @type {ChatItem} */ ({
                  id: sid,
                  kind: "assistant",
                  parts: parseAssistantMessage(extractPlan(partial).text || "…"),
                  ts: Date.now(),
                })
              : i,
          );
        streamingId = null;
        streamingContent = "";
        _pendingStreamContent = "";
      } else {
        items = items.filter(
          (i) => i.kind !== "thinking" && i.kind !== "executing",
        );
      }
    } finally {
      // Whatever went wrong above, the spinner and Stop must not outlive the turn.
      abortController = null;
      loading = false;
      openResultId = null;
    }
    await tick();
    inputRef?.focus();
    drainQueue();
  }

  // ── Local agent CLIs ──────────────────────────────────────────────────────
  /** Wall clock for a live "Working for 12s"; ticks only while a turn runs. */
  let nowMs = $state(Date.now());
  $effect(() => {
    if (!loading) return;
    nowMs = Date.now();
    const timer = setInterval(() => (nowMs = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  /**
   * What the list shows for a chat. Chats saved before titles skipped small
   * talk are all called "hi"; they get a title from their messages here,
   * without rewriting what is stored.
   * @param {{ title?: string, createdAt?: number, items?: unknown[] }} conv
   */
  function displayTitle(conv) {
    if (conv.title && !isPlaceholderTitle(conv.title)) return conv.title;
    const texts = (conv.items ?? [])
      .filter((/** @type {any} */ i) => i?.kind === "user")
      .map((/** @type {any} */ i) => String(i.text ?? ""));
    return texts.length ? conversationTitle(texts, conv.createdAt) : conv.title || "New chat";
  }

  /**
   * An icon that says what a suggestion does. They used to be handed out by
   * position, so "Schema overview" wore a bar chart and duplicates a book.
   * @param {string} label
   */
  function suggestionIcon(label) {
    const l = label.toLowerCase();
    if (/duplicate/.test(l)) return Copy;
    if (/foreign|relationship/.test(l)) return Link2;
    if (/size/.test(l)) return HardDrive;
    if (/count/.test(l)) return Hash;
    if (/recent|activity|updated/.test(l)) return Clock;
    if (/group|distribution|breakdown/.test(l)) return BarChart2;
    if (/schema|overview/.test(l)) return Layers;
    if (/browse|explore|rows/.test(l)) return Table2;
    if (/find|search/.test(l)) return Search;
    return Sparkles;
  }

  /** Docked chats keep a tab strip; full mode lists chats in its sidebar. */
  const showTabBar = $derived(mode !== "full" && (openTabs.length > 0 || !activeConvId));

  /**
   * Items from this index on arrived during this visit and slide in; the ones
   * a saved conversation opened with appear at once, so switching chats does
   * not replay forty entrances.
   */
  let enterFrom = $state(0);

  /**
   * Whether a fold is open: what the user chose, else open while it runs or
   * while a step is still failing, so the live timeline and any error are in
   * view without a click.
   * @param {Extract<ChatItem, { kind: 'work' }>} item @param {boolean} running
   */
  function isWorkOpen(item, running) {
    const chosen = workOpenChoice.get(item.id);
    if (chosen !== undefined) return chosen;
    return running || groupRetries(item.steps).some((g) => g.status === "error");
  }
  /** Folds the user opened or closed by hand. */
  let workOpenChoice = $state(/** @type {Map<string, boolean>} */ (new Map()));
  /** @param {Extract<ChatItem, { kind: 'work' }>} item @param {boolean} running */
  function toggleWorkItem(item, running) {
    const next = new Map(workOpenChoice);
    next.set(item.id, !isWorkOpen(item, running));
    workOpenChoice = next;
  }

  /** Steps whose SQL and result the user expanded. */
  let openSteps = $state(/** @type {Set<string>} */ (new Set()));
  /** @param {string} id */
  function toggleStep(id) {
    const next = new Set(openSteps);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    openSteps = next;
  }

  // ── The built-in model's step timeline ────────────────────────────────────
  /** The running turn's fold; null until its first tool call. */
  let builtinWorkId = /** @type {string | null} */ (null);
  /** A plan written before the fold exists, attached when it is made. */
  let pendingPlan = /** @type {string[] | null} */ (null);

  /** @param {string} id @param {(w: any) => any} fn */
  function patchWorkItem(id, fn) {
    items = items.map((i) => (i.id === id && i.kind === "work" ? fn(i) : i));
  }

  /** @param {string[]} plan */
  function acceptPlan(plan) {
    if (builtinWorkId && items.some((i) => i.id === builtinWorkId)) {
      patchWorkItem(builtinWorkId, (w) => (w.plan ? w : { ...w, plan }));
    } else pendingPlan = plan;
  }

  /**
   * The turn's fold, made on first use - in place of `replaceId` (the
   * narration it absorbs) or at the end - and its clock resumed if the model
   * paused to write.
   * @param {string | null} [replaceId]
   */
  function ensureBuiltinWork(replaceId = null) {
    if (builtinWorkId && items.some((i) => i.id === builtinWorkId)) {
      patchWorkItem(builtinWorkId, (w) =>
        w.endedAt === null ? w : { ...w, startedAt: w.startedAt + (Date.now() - w.endedAt), endedAt: null },
      );
      if (replaceId) items = items.filter((i) => i.id !== replaceId);
      return builtinWorkId;
    }
    const id = uid();
    const work = /** @type {ChatItem} */ ({
      id,
      kind: "work",
      startedAt: Date.now(),
      endedAt: null,
      steps: [],
      ...(pendingPlan ? { plan: pendingPlan } : {}),
    });
    pendingPlan = null;
    const at = replaceId ? items.findIndex((i) => i.id === replaceId) : -1;
    if (at >= 0) items.splice(at, 1, work);
    else items.push(work);
    builtinWorkId = id;
    return id;
  }

  /** @param {WorkStep} step */
  function addBuiltinStep(step) {
    const wid = ensureBuiltinWork();
    const thinkIdx = items.findIndex((i) => i.kind === "thinking");
    if (thinkIdx >= 0) items.splice(thinkIdx, 1);
    patchWorkItem(wid, (w) => ({ ...w, steps: [...w.steps, step] }));
  }

  /** @param {string} stepId @param {Partial<WorkStep>} patch */
  function patchBuiltinStep(stepId, patch) {
    if (!builtinWorkId) return;
    patchWorkItem(builtinWorkId, (w) => ({
      ...w,
      steps: w.steps.map((/** @type {WorkStep} */ s) => (s.id === stepId ? { ...s, ...patch } : s)),
    }));
  }

  /** Stop the fold's clock while the model writes its answer. */
  function pauseBuiltinWork() {
    if (builtinWorkId) patchWorkItem(builtinWorkId, (w) => (w.endedAt === null ? { ...w, endedAt: Date.now() } : w));
  }

  /**
   * Close every fold still open when a turn ends or is stopped: the clock
   * stops, and a step that never answered says so instead of spinning on.
   */
  function settleOpenWork() {
    const now = Date.now();
    items = items.map((i) =>
      i.kind === "work" && (i.endedAt === null || i.steps.some((s) => s.status === "running"))
        ? {
            ...i,
            endedAt: i.endedAt ?? now,
            steps: i.steps.map((s) =>
              s.status === "running" ? { ...s, status: /** @type {const} */ ("error"), error: "Stopped before it finished.", endedAt: now } : s,
            ),
          }
        : i,
    );
  }

  /** @param {number} ms */
  function formatElapsed(ms) {
    const s = Math.max(1, Math.round(ms / 1000));
    return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
  }

  /** "2 queries · 1 schema read". @param {WorkStep[]} steps */
  function workSummary(steps) {
    const queries = steps.filter((s) => s.op === "query").length;
    const web = steps.filter((s) => s.op === "web").length;
    const reads = steps.filter((s) => s.op !== "query" && s.op !== "web" && s.op !== "note").length;
    return [
      queries ? `${queries} ${queries === 1 ? "query" : "queries"}` : "",
      web ? `${web} web ${web === 1 ? "lookup" : "lookups"}` : "",
      reads ? `${reads} schema ${reads === 1 ? "read" : "reads"}` : "",
    ].filter(Boolean).join(" · ");
  }


  /** The agent session behind this chat: its backend key and what started it. */
  let agentSession = /** @type {{ key: string, agent: string, model: string, effort: string, access: string, ready: Promise<void> } | null} */ (null);
  /** The CLI's own session id for the open conversation, to resume it later. */
  let agentNative = /** @type {{ agent: string, id: string } | null} */ (null);
  /** Receives the running turn's events; null between turns. */
  let agentOnEvent = /** @type {((ev: any) => void) | null} */ (null);
  /** Ends the running turn early (Stop, or a switch of conversation). */
  let agentSettle = /** @type {(() => void) | null} */ (null);

  function stopAgentSession() {
    if (agentSession) void agentStop(agentSession.key).catch(() => {});
    agentSession = null;
  }

  function interruptAgentTurn() {
    if (agentSession && agentOnEvent) void agentInterrupt(agentSession.key).catch(() => {});
    agentOnEvent = null;
    agentSettle?.();
    agentSettle = null;
  }

  /**
   * The session to send to, started when there is none or the agent or model
   * changed. A restart resumes the CLI's own session, so the agent keeps the
   * conversation it had.
   * @param {string} agent @param {string} model
   */
  async function ensureAgentSession(agent, model) {
    const effort = AGENTS_WITH_EFFORT.has(agent) ? $agentPrefs.effort : "";
    // A connection opened read-only stays read-only for the agent too.
    /** @type {import('$lib/local-agents.js').AgentAccess} */
    const access = isReadOnly() ? "read" : $agentPrefs.access;
    const s = agentSession;
    const webAccess = $appAgentWebAccess;
    if (s?.agent === agent && s.model === model && s.effort === effort && s.access === access && s.webAccess === webAccess) {
      // Started already, or still starting from the composer's focus: wait for
      // that start rather than racing it with a second one.
      await s.ready;
      return s.key;
    }
    stopAgentSession();
    const key = activeConvId ?? `draft-${uid()}`;
    /** @type {(v?: unknown) => void} */
    let started = () => {};
    /** @type {(e: unknown) => void} */
    let failed = () => {};
    /** @type {Promise<void>} */
    const ready = new Promise((res, rej) => { started = res; failed = rej; });
    // A failed start is reported by the send that awaits it, not as unhandled.
    ready.catch(() => {});
    const session = { key, agent, model, effort, access, webAccess, ready };
    agentSession = session;
    const tables = (schemaContext.tables ?? [])
      .map((/** @type {any} */ t) => (typeof t === "string" ? t : t?.name))
      .filter(Boolean);
    try {
      await agentStart(
        {
          conversationId: key,
          agent,
          model,
          systemPrompt: agentSystemPrompt({
            dbType: schemaContext.dbType,
            activeSchema: schemaContext.activeSchema,
            activeTable: schemaContext.activeTable,
            tables,
            access,
            webAccess,
          }),
          resume: agentNative?.agent === agent ? agentNative.id : null,
          effort: effort || null,
          access,
          // The launch flag that actually gives the agent its web tools; the
          // prompt above only tells it they exist.
          webAccess,
        },
        (ev) => {
          if (agentSession !== session) return;
          const native = ev.Started?.native_session_id;
          if (native) agentNative = { agent, id: native };
          if (ev.Fatal) agentSession = null;
          agentOnEvent?.(ev);
        },
      );
      started();
    } catch (e) {
      if (agentSession === session) agentSession = null;
      failed(e);
      throw e;
    }
    return key;
  }

  /**
   * Start the agent while the user is still typing, as T3 and deck do: the
   * CLI's boot (a second or two for Claude Code) is paid before Send, not after.
   */
  function prewarmAgent() {
    const agent = profileAgent($aiSettings);
    if (!agent || loading) return;
    ensureAgentSession(agent, $aiSettings.model ?? "").catch(() => {});
  }

  /**
   * One turn with a local agent, drawn with the same items as the HTTP path:
   * streamed text, a query card per Stroke tool call, result tables.
   * Resolves with the reply's text.
   * @param {string} agent @param {string} model @param {string} text
   * @param {() => boolean} disowned
   * @returns {Promise<string>}
   */
  async function runAgentTurn(agent, model, text, disowned) {
    const key = await ensureAgentSession(agent, model);
    let content = "";
    let reply = "";
    let itemId = /** @type {string | null} */ (null);
    /** The turn's one "Worked for" fold, made at its first tool call or note. */
    let workId = /** @type {string | null} */ (null);
    /** @type {Map<string, { workId: string, name: string, detail: string }>} */
    const tools = new Map();
    /** @param {string} id @param {(w: any) => any} fn */
    const patchWork = (id, fn) => {
      items = items.map((i) => (i.id === id && i.kind === "work" ? fn(i) : i));
    };
    /** Stop the fold's clock while the agent writes; the next call resumes it. */
    const closeWork = () => {
      if (!workId) return;
      patchWork(workId, (w) => (w.endedAt === null ? { ...w, endedAt: Date.now() } : w));
    };
    const dropThinking = () => {
      const i = items.findIndex((x) => x.kind === "thinking");
      if (i >= 0) items.splice(i, 1);
    };
    /** Finish the streamed text so a tool card can follow it. */
    const closeText = () => {
      closeWork();
      if (!itemId) return;
      flushStreamingContent();
      const id = itemId;
      const parts = parseAssistantMessage(content);
      items = items.flatMap((i) => (i.id === id ? replyItems(id, parts) : [i]));
      reply += (reply ? "\n\n" : "") + content;
      itemId = null;
      streamingId = null;
      streamingContent = "";
      _pendingStreamContent = "";
      content = "";
    };
    /**
     * Text written before a tool call was narration ("I'll check the schema"),
     * not the answer: it moves into the fold, so only the reply that ends the
     * turn stays in the transcript, as T3 does.
     */
    const foldText = () => {
      if (!itemId) return;
      flushStreamingContent();
      const id = itemId;
      const note = content.trim();
      itemId = null;
      streamingId = null;
      streamingContent = "";
      _pendingStreamContent = "";
      content = "";
      if (!workId) {
        // The fold takes the narration's place in the transcript.
        const wid = uid();
        workId = wid;
        items = items.map((i) =>
          i.id === id ? /** @type {ChatItem} */ ({ id: wid, kind: "work", startedAt: Date.now(), endedAt: null, steps: [] }) : i,
        );
      } else {
        items = items.filter((i) => i.id !== id);
      }
      if (note) {
        /** @type {WorkStep} */
        const step = { id: uid(), tool: "note", op: "note", detail: note, status: "done" };
        patchWork(workId, (w) => ({ ...w, steps: [...w.steps, step] }));
      }
    };
    /** @param {any} call */
    const onToolCall = (call) => {
      const tool = strokeToolName(String(call.name ?? ""));
      const input = call.input ?? {};
      /** @type {WorkStep['op']} */
      const op = tool === "execute_sql" || tool === "explain_query" ? "query"
        : tool === "describe_table" ? "describe"
        : isWebTool(tool) ? "web" : "schema";
      const detail = op === "query" ? String(input.sql ?? "")
        : op === "describe" ? String(input.table ?? "")
        : op === "web" ? String(input.query ?? input.url ?? input.prompt ?? "")
        : tool.replace(/_/g, " ");
      const known = tools.get(call.id);
      if (known) {
        // The same call again with its full input.
        known.detail = detail || known.detail;
        patchWork(known.workId, (w) => ({
          ...w,
          steps: w.steps.map((/** @type {WorkStep} */ s) => (s.id === call.id ? { ...s, detail: known.detail } : s)),
        }));
        return;
      }
      foldText();
      dropThinking();
      if (!workId) {
        workId = uid();
        items.push(/** @type {ChatItem} */ ({ id: workId, kind: "work", startedAt: Date.now(), endedAt: null, steps: [] }));
      } else {
        // Working again after a note: the clock runs on from where it paused.
        patchWork(workId, (w) => (w.endedAt === null ? w : { ...w, startedAt: w.startedAt + (Date.now() - w.endedAt), endedAt: null }));
      }
      const wid = workId;
      tools.set(call.id, { workId: wid, name: tool, detail });
      /** @type {WorkStep} */
      const step = { id: String(call.id), tool, op, detail, status: "running" };
      patchWork(wid, (w) => ({ ...w, steps: [...w.steps, step] }));
      scrollBottomSoon();
    };
    /** @param {any} res */
    const onToolResult = (res) => {
      const t = tools.get(res.id);
      if (!t) return;
      const out = toolOutputText(res.output);
      patchWork(t.workId, (w) => ({
        ...w,
        steps: w.steps.map((/** @type {WorkStep} */ s) =>
          s.id === String(res.id)
            ? { ...s, status: res.is_error ? "error" : "done", ...(res.is_error ? { error: cleanToolError(out).slice(0, 400) } : {}) }
            : s,
        ),
      }));
      if (t.name === "execute_sql" && !res.is_error && isDdl(t.detail)) onschemadirty();
      const parsed = t.name === "execute_sql" && !res.is_error ? parseSqlResult(out) : null;
      // Rows still get their table; a failure is already shown in the fold.
      if (parsed && parsed.total > 0) {
        const id = uid();
        items.push(/** @type {ChatItem} */ ({ id, kind: "result", sql: t.detail, columns: parsed.columns, rows: parsed.rows.slice(0, AI_DISPLAY_ROWS), total: parsed.total, error: null }));
        autoOpenResult(id);
      }
    };

    await new Promise((resolve, reject) => {
      // A tool that never answers (the database stalls, or the MCP server
      // stops responding) used to leave the steps spinning forever with no
      // word about why. Say which step it is waiting on after 30s; after 3
      // minutes interrupt the agent and end the turn with the reason.
      let lastEventAt = Date.now();
      const runningSteps = () => [...tools.entries()].filter(([id, t]) =>
        items.some((i) => i.id === t.workId && i.kind === "work" && /** @type {any} */ (i).steps.some((/** @type {WorkStep} */ s) => s.id === String(id) && s.status === "running")));
      const watchdog = setInterval(() => {
        const quiet = Date.now() - lastEventAt;
        const waiting = runningSteps();
        if (!waiting.length) return;
        const what = waiting.length === 1 ? (waiting[0][1].detail || waiting[0][1].name.replace(/_/g, " ")) : `${waiting.length} steps`;
        if (quiet >= 180_000) {
          for (const [id, t] of waiting) {
            patchWork(t.workId, (w) => ({
              ...w,
              steps: w.steps.map((/** @type {WorkStep} */ st) => (st.id === String(id) ? { ...st, status: "error", error: "No answer in 3 minutes." } : st)),
            }));
          }
          void agentInterrupt(key).catch(() => {});
          error = `No answer from ${what} in 3 minutes. The database may be busy or unreachable; check Logs, then try again.`;
          closeText();
          closeWork();
          settle(null);
        } else if (quiet >= 30_000) {
          aiStatusHint = `Still waiting on ${what}… Stop ends it.`;
        }
      }, 5_000);
      const settle = (/** @type {Error | null} */ err) => {
        clearInterval(watchdog);
        agentOnEvent = null;
        agentSettle = null;
        if (err) reject(err);
        else resolve(undefined);
      };
      agentSettle = () => settle(null);
      agentOnEvent = (ev) => {
        lastEventAt = Date.now();
        if (aiStatusHint.startsWith("Still waiting on")) aiStatusHint = "";
        if (disowned()) return settle(null);
        if (ev.Text) {
          aiStatusHint = "";
          content += ev.Text.delta;
          if (!itemId) {
            closeWork();
            dropThinking();
            itemId = uid();
            streamingId = itemId;
            items.push(/** @type {ChatItem} */ ({ id: itemId, kind: "streaming" }));
          }
          scheduleStreamingUpdate(content);
        } else if (ev.ToolCall) onToolCall(ev.ToolCall);
        else if (ev.ToolResult) onToolResult(ev.ToolResult);
        else if (ev.TurnFailed) error = ev.TurnFailed.message;
        else if (ev.TurnEnd) {
          closeText();
          closeWork();
          settle(null);
        } else if (ev.Fatal) {
          closeText();
          closeWork();
          settle(new Error(ev.Fatal.message));
        }
      };
      agentSend(key, text).catch(async (e) => {
        if (String(e) !== "NO_SESSION") return settle(e instanceof Error ? e : new Error(String(e)));
        // The CLI exited between turns: start it again, resuming, and resend.
        agentSession = null;
        try {
          await agentSend(await ensureAgentSession(agent, model), text);
        } catch (e2) {
          settle(e2 instanceof Error ? e2 : new Error(String(e2)));
        }
      });
    });
    return reply + (content ? (reply ? "\n\n" : "") + content : "");
  }

  /** Max rows fetched from DB per AI tool call - prevents OOM on large tables */
  const AI_ROW_LIMIT = 500;
  /** Max rows kept in chat state for display */
  const AI_DISPLAY_ROWS = 200;

  /**
   * Append LIMIT to SELECT/CTE queries that lack one.
   * DML/DDL pass through unchanged.
   * @param {string} sql
   * @returns {{ sql: string, capped: boolean }}
   */
  /**
   * describe_table / get_schema for schemaless engines: the fields seen in a
   * few sampled records of each collection.
   * @param {Record<string, any>} args @param {string} tool
   */
  async function describeNosql(args, tool) {
    const engine = String(schemaContext.dbType);
    const schema = String(args.schema ?? schemaContext.activeSchema ?? "");
    const names = tool === "get_schema" && !args.table
      ? (schemaContext.tables ?? []).slice(0, 10).map((t) => t.name)
      : [String(args.table ?? "")];
    const out = await Promise.all(names.map(async (n) => {
      const q = sampleQuery(engine, schema, n, 5);
      if (!q) return { collection: n, note: "Sampling would scan; query it by its key to see records." };
      try {
        return describeFromSample(n, await executeSql(q));
      } catch (e) {
        return { collection: n, error: e instanceof Error ? e.message : String(e) };
      }
    }));
    return JSON.stringify(names.length === 1 ? out[0] : { collections: out });
  }

  function guardSelectLimit(sql) {
    // Only SQL and CQL take a LIMIT clause; PartiQL and the shell languages don't.
    const engine = schemaContext.dbType;
    if (queryLanguageFor(engine) && engine !== "cassandra") return { sql: sql.trimEnd().replace(/;+$/, ""), capped: false };
    // Strip trailing semicolons first - appending "\nLIMIT N" after a ";" produces
    // a bare LIMIT statement that PostgreSQL rejects with a syntax error.
    const cleaned = sql.trimEnd().replace(/;+$/, "");
    const t = cleaned.trimStart();
    if (!/^(with\b|select\b)/i.test(t)) return { sql: cleaned, capped: false };
    if (/\blimit\s+\d/i.test(t)) return { sql: cleaned, capped: false };
    return { sql: `${cleaned}\nLIMIT ${AI_ROW_LIMIT}`, capped: true };
  }

  /**
   * @param {number} depth
   * @param {AbortController} ctrl this turn's own controller. It used to read
   *   the shared one, so a stopped turn that was still settling picked up the
   *   NEXT turn's live controller and carried on as if nothing had happened.
   */
  async function runAiTurn(depth, ctrl) {
    if (depth > 40)
      throw new Error("Too many AI iterations, aborting runaway execution");
    // The chain can resume here after a declined confirm, so check first.
    if (ctrl.signal.aborted)
      throw Object.assign(new Error("Aborted"), { name: "AbortError" });

    // Space out follow-up turns after tool calls to avoid burst rate limits
    if (depth > 0) {
      await new Promise((r) => setTimeout(r, 300));
      if (ctrl.signal.aborted)
        throw Object.assign(new Error("Aborted"), { name: "AbortError" });
    }

    let fullContent = "";
    /** @type {import('$lib/ai.js').ToolCall[]} */
    let toolCalls = [];
    /** ID of the streaming placeholder item created on first text token */
    let itemId = /** @type {string | null} */ (null);

    for await (const chunk of chatCompletionStream(
      settings,
      [{ role: "system", content: turnSystemPrompt }, ...repairToolPairs(apiHistory)],
      turnTools,
      ctrl.signal,
      ({ attempt, waitMs, status, model }) => {
        if (ctrl.signal.aborted) return;
        if (model) {
          aiStatusHint = `Busy, trying ${modelDisplayName({ model }).split(",")[0]} instead…`;
          return;
        }
        const sec = Math.ceil(waitMs / 1000);
        const why = status === 429 ? "Rate limited" : "The AI service is busy";
        aiStatusHint = `${why}, retrying in ${sec}s (attempt ${attempt}/${MAX_AI_RETRIES})…`;
      },
    )) {
      // A provider can keep streaming after Stop; nothing more reaches the screen.
      if (ctrl.signal.aborted) break;
      if (chunk.textDelta) {
        aiStatusHint = "";
        fullContent += chunk.textDelta;
        if (!itemId) {
          itemId = uid();
          streamingId = itemId;
          // Remove the thinking indicator the moment the first token arrives, then append streaming placeholder
          const thinkIdx = items.findIndex((i) => i.kind === "thinking");
          if (thinkIdx >= 0) items.splice(thinkIdx, 1);
          items.push(
            /** @type {ChatItem} */ ({ id: itemId, kind: "streaming" }),
          );
        }
        // No scroll here: chunks arrive far faster than the ~90ms commit, and
        // reading scrollHeight forces a synchronous layout of the whole
        // conversation. AiMarkdown calls scrollBottomSoon via `onrender`, so the
        // view sticks to the bottom exactly when the content has actually grown.
        scheduleStreamingUpdate(fullContent);
      }
      if (chunk.toolStart && /^render_(chart|diagram)$/.test(chunk.toolStart)) {
        // The spec streams for seconds; show what is being made right away.
        const thinkIdx = items.findIndex((i) => i.kind === "thinking");
        if (thinkIdx >= 0) items.splice(thinkIdx, 1);
        items.push(/** @type {ChatItem} */ ({ id: uid(), kind: "generating", what: chunk.toolStart === "render_chart" ? "chart" : "diagram" }));
        void scrollBottom();
      }
      if (chunk.toolCalls) {
        toolCalls = chunk.toolCalls;
      }
    }
    // The real chart/diagram (or an error) replaces the generating cards.
    const dropGenerating = () => {
      for (let k = items.length - 1; k >= 0; k--) if (items[k].kind === "generating") items.splice(k, 1);
    };

    // Bail out immediately if the user stopped generation - stop() already finalized UI
    if (ctrl.signal.aborted) {
      dropGenerating();
      throw Object.assign(new Error("Aborted"), { name: "AbortError" });
    }

    // A tool call written into the reply as JSON instead of made: make it, and
    // keep the JSON out of the transcript.
    if (!toolCalls.length && fullContent && turnTools?.length) {
      const names = turnTools.map((t) => /** @type {any} */ (t)?.function?.name).filter(Boolean);
      const found = extractTextToolCalls(fullContent, names);
      if (found.toolCalls.length) {
        fullContent = found.text;
        toolCalls = found.toolCalls;
        scheduleStreamingUpdate(fullContent);
      }
    }

    // Flush any buffered streaming content before finalizing
    flushStreamingContent();

    // A <plan> opens a multi-step turn: it becomes the fold's checklist, and
    // the reply keeps the rest. History keeps the plan, so the model sees it.
    const { plan, text: shownText } = extractPlan(fullContent);
    if (plan && toolCalls.length > 0) acceptPlan(plan);

    // Promote the streaming placeholder to a finalized assistant item
    if (itemId && streamingId) {
      streamingId = null;
      streamingContent = "";
      _pendingStreamContent = "";
      const parts = parseAssistantMessage(shownText);
      if (toolCalls.length > 0 && parts.every((p) => p.type === "text")) {
        // Text before a tool call is narration ("Creating the tables first"),
        // not the answer: it moves into the fold, and only the reply that ends
        // the turn stays in the transcript, as the local agents' turns do.
        const wid = ensureBuiltinWork(itemId);
        const note = shownText.trim();
        if (note) {
          patchWorkItem(wid, (w) => ({
            ...w,
            steps: [...w.steps, /** @type {WorkStep} */ ({ id: uid(), tool: "note", op: "note", detail: note, status: "done" })],
          }));
        }
      } else {
        items = items.flatMap((i) => (i.id === itemId ? replyItems(itemId, parts) : [i]));
      }
    }
    if (toolCalls.length === 0) pauseBuiltinWork();

    dropGenerating();
    if (toolCalls.length > 0) {
      // Drop the "Thinking…" placeholder so the executing rows are the live indicator.
      const thinkIdx = items.findIndex((i) => i.kind === "thinking");
      if (thinkIdx >= 0) items.splice(thinkIdx, 1);
      apiHistory.push({
        role: "assistant",
        content: fullContent || null,
        tool_calls: toolCalls,
      });
      for (const call of toolCalls) {
        await runToolCall(call, ctrl);
      }
      if (ctrl.signal.aborted)
        throw Object.assign(new Error("Aborted"), { name: "AbortError" });
      // The next turn is the model interpreting the tool output - show that as a
      // distinct phase instead of a generic "Thinking…". Cleared when it streams.
      aiStatusHint = "Reviewing results…";
      items.push(/** @type {ChatItem} */ ({ id: uid(), kind: "thinking" }));
      scrollBottomSoon();
      await runAiTurn(depth + 1, ctrl);
    } else if (fullContent) {
      apiHistory.push({ role: "assistant", content: fullContent });
      // Fallback: if no streaming item was created (non-streaming endpoint), add it now
      if (!itemId) {
        items.push(...replyItems(uid(), parseAssistantMessage(shownText)));
        await scrollBottom();
      }
    } else {
      // No text, no tool call, no transport error. This used to fall off the end
      // of the function: the thinking indicator was cleared in `send`'s finally
      // block and the turn vanished, which looks exactly like the app ignoring
      // the question. Say what happened and name the model, because the fix is
      // almost always to switch it.
      throw new Error(
        `${settings.model || "The model"} returned an empty response. ` +
          "Nothing was streamed back - no text and no tool call. " +
          "Try another model, or send the message again.",
      );
    }
  }

  /**
   * @param {import('$lib/ai.js').ToolCall} call
   * @param {AbortController} ctrl the turn's own controller
   */
  async function runToolCall(call, ctrl) {
    // The tool loop can resume here after an abort resolves a pending confirm -
    // answer the call as cancelled instead of executing it for a dead turn.
    if (ctrl.signal.aborted) {
      apiHistory.push({
        role: "tool",
        tool_call_id: call.id,
        content: JSON.stringify({ cancelled: true }),
      });
      return;
    }
    const callKey = `${call.function.name}:${call.function.arguments}`;
    if (executedCalls.has(callKey)) {
      apiHistory.push({
        role: "tool",
        tool_call_id: call.id,
        content: JSON.stringify({
          error:
            "Duplicate call, this exact operation already ran this turn. Use the previous result instead of calling again.",
        }),
      });
      return;
    }
    const prior = failureTracker.get(callKey);
    if (prior && prior.count >= 2) {
      apiHistory.push({
        role: "tool",
        tool_call_id: call.id,
        content: JSON.stringify({
          error: `This call has already failed ${prior.count} times this turn. Do NOT retry it again.`,
          last_error: prior.lastError,
          // The old wording here was "explain to the user why this cannot be
          // done", which asked the model to account for a failure it had no
          // information about - so it invented one, and a plain "no such table"
          // got reported to the user as a database permission problem. Report
          // the error; do not theorise about its cause.
          instruction:
            "Either try a genuinely different approach (a different tool, or SQL for THIS engine), " +
            "or tell the user the operation failed and quote last_error verbatim. " +
            "Do NOT guess at a cause you have not verified - in particular, never claim a permissions, " +
            "access, or authentication problem unless last_error actually says so.",
        }),
      });
      return;
    }
    executedCalls.add(callKey);

    let toolResult = "";
    try {
      const args = JSON.parse(call.function.arguments || "{}");

      if (call.function.name === "execute_sql") {
        const sql = String(args.sql ?? "").trim();
        if (sql && isReadOnly() && isWriteFor(sql, schemaContext.dbType)) {
          // Refuse in the tool channel: the model needs to hear *why*, or it
          // retries the same write until it hits the retry cap.
          const now = Date.now();
          addBuiltinStep({ id: String(call.id || uid()), tool: "execute_sql", op: "query", detail: sql, status: "error", error: "Not run: this connection is open in read-only mode.", startedAt: now, endedAt: now });
          apiHistory.push({
            role: "tool",
            tool_call_id: call.id,
            content: JSON.stringify({
              error:
                "This connection is open in read-only mode. Writing statements are blocked - propose the SQL to the user instead of running it.",
            }),
          });
          return;
        }
        if (!sql) {
          apiHistory.push({
            role: "tool",
            tool_call_id: call.id,
            content: JSON.stringify({ error: "Empty SQL provided" }),
          });
          return;
        }
        if (isDestructiveSql(sql)) {
          const confirmed = await waitForConfirm(sql);
          if (!confirmed) {
            apiHistory.push({
              role: "tool",
              tool_call_id: call.id,
              content: JSON.stringify({
                cancelled: true,
                message: "User declined this operation.",
              }),
            });
            return;
          }
        }
        // Another database on the same server: the model passes `database`
        // (a database it just created, or one the user named). It runs over a
        // short-lived connection with the same login, never in the current one.
        const otherDb = String(args.database ?? "").trim();
        const crossDb = !!otherDb && otherDb !== (connectionConfig?.database ?? database);
        if (crossDb && !CROSS_DB_ENGINES.has(String(connectionConfig?.type ?? ""))) {
          apiHistory.push({
            role: "tool",
            tool_call_id: call.id,
            content: JSON.stringify({ error: `This engine can't open another database by name. Run it in the connected database (${connectionConfig?.database || database || "this one"}) or ask the user to connect to ${otherDb}.` }),
          });
          return;
        }
        const { sql: guardedSql, capped: frontendCapped } =
          guardSelectLimit(sql);
        // One row in the turn's step timeline, not a card of its own: running,
        // then done with its time and row count, or failed with the error.
        const stepId = String(call.id || uid());
        addBuiltinStep({ id: stepId, tool: "execute_sql", op: "query", detail: crossDb ? `-- in ${otherDb}\n${sql}` : sql, status: "running", startedAt: Date.now() });
        scrollBottomSoon();
        try {
          const data = crossDb
            ? await executeSqlOnConnection(/** @type {any} */ ({ ...connectionConfig, database: otherDb }), guardedSql)
            : await executeSql(guardedSql);
          if (isDdl(sql)) onschemadirty();
          const cols = data.columns ?? [];
          const rows = data.rows ?? [];
          const total = data.rowCount ?? rows.length;
          const backendCapped =
            typeof data.message === "string" &&
            data.message.startsWith("Showing first");
          patchBuiltinStep(stepId, {
            status: "done",
            endedAt: Date.now(),
            affected: cols.length ? rows.length : total,
            message: typeof data.message === "string" ? data.message : null,
          });
          // Rows get their table after the timeline, as before. A write's
          // "3 row(s) affected" lives on its step; the model still gets it below.
          if (cols.length > 0 && total > 0) {
            const resultId = uid();
            items.push(/** @type {ChatItem} */ ({
              id: resultId,
              kind: "result",
              sql,
              columns: cols,
              rows: rows.slice(0, AI_DISPLAY_ROWS),
              total,
              error: null,
              capped: frontendCapped,
            }));
            autoOpenResult(resultId);
          }
          scrollBottomSoon();
          // Return rows as objects (not arrays) so the AI can pass `rows`
          // directly to render_chart without needing to transform the data.
          const colNames = cols.map((c) => c.name);
          const rowObjects = rows
            .slice(0, 60)
            .map((r) =>
              Object.fromEntries(
                colNames.map((n, i) => [n, /** @type {any[]} */ (r)[i]]),
              ),
            );
          toolResult = JSON.stringify({
            columns: colNames,
            rows: rowObjects,
            total_rows: total,
            ...(backendCapped
              ? {
                  notice:
                    data.message ??
                    "Results capped. Use WHERE or LIMIT to narrow results.",
                }
              : {}),
            message: data.message ?? null,
          });
        } catch (sqlErr) {
          const msg = String(sqlErr).replace(/^Error:\s*/, "");
          const hint = classifyDbError(msg, sql);
          // The failure stays in the timeline with the database's own words,
          // in red under the step: the user sees what the model is recovering
          // from, and the retry folds under it.
          patchBuiltinStep(stepId, { status: "error", endedAt: Date.now(), error: msg });
          const existing = failureTracker.get(callKey) ?? {
            count: 0,
            lastError: "",
          };
          failureTracker.set(callKey, {
            count: existing.count + 1,
            lastError: msg,
          });
          // The full error text reaches the model, plus a hint when one
          // applies, so it can fix the statement instead of guessing.
          toolResult = JSON.stringify({
            error: msg,
            ...(hint ? { hint } : {}),
            attempt: existing.count + 1,
            instruction: "Fix the cause named in `error` and call execute_sql again with corrected SQL. Do not resend the same statement.",
          });
        }
      } else if (
        call.function.name === "web_search" ||
        call.function.name === "fetch_page"
      ) {
        // Re-checked here, not just at tool-advertisement time: the user can
        // turn web access off mid-conversation, and a model that already saw
        // the tool will keep calling it from history.
        if (!$appAgentWebAccess) {
          apiHistory.push({
            role: "tool",
            tool_call_id: call.id,
            content: JSON.stringify({
              error:
                "Web access is turned off in Settings → Agent. Answer from the database and your own knowledge instead, and do not call this tool again this conversation.",
            }),
          });
          return;
        }
        const isSearch = call.function.name === "web_search";
        const label = isSearch
          ? String(args.query ?? "").trim()
          : String(args.url ?? "").trim();
        const webId = uid();
        items.push(
          /** @type {ChatItem} */ ({
            id: webId,
            kind: "web",
            mode: isSearch ? "search" : "fetch",
            label,
            hits: [],
            error: null,
            loading: true,
          }),
        );
        await scrollBottom();
        /** @param {Partial<any>} patch */
        const patchWeb = (patch) => {
          const i = items.findIndex((x) => x.id === webId);
          if (i >= 0) items[i] = { ...items[i], ...patch, loading: false };
        };
        try {
          if (isSearch) {
            const hits = await aiWebSearch(label, Number(args.limit) || 5);
            patchWeb({ hits });
            toolResult = JSON.stringify(
              hits.length
                ? { results: hits }
                : {
                    results: [],
                    note: "No results. Try different wording, or answer from what you already know.",
                  },
            );
          } else {
            const text = await aiFetchPage(label);
            patchWeb({ hits: [{ title: label, url: label, snippet: "" }] });
            toolResult = JSON.stringify({ url: label, text });
          }
        } catch (webErr) {
          const msg = String(webErr);
          patchWeb({ error: humanizeDbError(msg) });
          const existing = failureTracker.get(callKey) ?? { count: 0, lastError: "" };
          failureTracker.set(callKey, { count: existing.count + 1, lastError: msg });
          toolResult = JSON.stringify({ error: msg });
        }
      } else if (call.function.name === "export_data") {
        const sql = String(args.sql ?? "").trim();
        const fmt =
          args.format === "json"
            ? "json"
            : args.format === "markdown" || args.format === "md"
              ? "md"
              : "csv";
        if (!sql) {
          apiHistory.push({ role: "tool", tool_call_id: call.id, content: JSON.stringify({ error: "Empty SQL provided" }) });
          return;
        }
        if (isDestructiveSql(sql)) {
          apiHistory.push({ role: "tool", tool_call_id: call.id, content: JSON.stringify({ error: "export_data only runs read-only SELECT/WITH queries." }) });
          return;
        }
        try {
          // Export the full result (no display cap) so large exports are complete.
          const data = await executeSql(sql);
          const cols = data.columns ?? [];
          const rows = data.rows ?? [];
          const total = rows.length;
          if (total === 0) {
            toolResult = JSON.stringify({ exported: false, message: "Query returned no rows to export." });
          } else {
            const LARGE = 20000;
            let content;
            /** @type {string | number | undefined} */
            let toastId;
            if (total > LARGE && fmt !== "md") {
              // Large export: keep the UI responsive + show progress, like the table export.
              toastId = toast.info(`Preparing ${fmt.toUpperCase()}: ${formatCompactCount(total)} rows…`, { description: "This can take a moment for large files", duration: 60 * 60 * 1000 });
              await new Promise((r) => setTimeout(r, 16));
              content = fmt === "json" ? await rowsToJsonAsync(cols, rows) : await rowsToCsvAsync(cols, rows);
            } else {
              content = fmt === "json" ? rowsToJson(cols, rows) : fmt === "md" ? rowsToMarkdown(cols, rows) : rowsToCsv(cols, rows);
            }
            const base = String(args.filename ?? "export").replace(/\.[a-z0-9]+$/i, "").trim() || "export";
            const filename = buildExportFilename(base, fmt);
            const saved = await saveExportFile(content, filename, fmt);
            if (toastId != null) toast.dismiss(toastId);
            if (saved) {
              toast.success(`Downloaded ${filename}`, { description: `${formatCompactCount(total)} rows exported` });
              toolResult = JSON.stringify({ exported: true, filename, format: fmt, row_count: total, message: `Exported ${total} rows to ${filename}, the file has been downloaded.` });
            } else {
              toolResult = JSON.stringify({ exported: false, message: "The user cancelled the save dialog." });
            }
          }
        } catch (err) {
          toolResult = JSON.stringify({ error: String(err) });
        }
      } else if (queryLanguageFor(schemaContext.dbType) && (call.function.name === "describe_table" || call.function.name === "get_schema")) {
        toolResult = await describeNosql(args, call.function.name);
      } else if (call.function.name === "describe_table") {
        const schema = String(
          args.schema ?? schemaContext.activeSchema,
        ).replace(/'/g, "''");
        const table = String(args.table ?? "").replace(/'/g, "''");
        const execId = uid();
        items.push(
          /** @type {ChatItem} */ ({
            id: execId,
            kind: "executing",
            sql: `${schema}.${table}`,
            op: "describe",
          }),
        );
        await scrollBottom();
        const dbType = schemaContext.dbType ?? "postgres";
        const isSqliteFamily =
          dbType === "sqlite" || dbType === "d1" || dbType === "libsql";
        let cols, rows, colObjs;
        if (isSqliteFamily) {
          const data = await executeSql(
            `PRAGMA table_info("${table.replace(/"/g, '""')}")`,
          );
          cols = data.columns ?? [];
          rows = data.rows ?? [];
          // PRAGMA columns: cid, name, type, notnull, dflt_value, pk
          const nameIdx = cols.findIndex((c) => c.name === "name"),
            typeIdx = cols.findIndex((c) => c.name === "type");
          const nnIdx = cols.findIndex((c) => c.name === "notnull"),
            dfltIdx = cols.findIndex((c) => c.name === "dflt_value");
          const pkIdx = cols.findIndex((c) => c.name === "pk");
          colObjs = rows.map((r) => ({
            name: r[nameIdx] ?? r[1],
            type: r[typeIdx] ?? r[2] ?? "text",
            nullable: (r[nnIdx] ?? r[3]) ? "NO" : "YES",
            default: r[dfltIdx] ?? r[4] ?? null,
            pk: !!(r[pkIdx] ?? r[5]),
          }));
        } else {
          const descSql =
            dbType === "mysql"
              ? `SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${schema}' AND TABLE_NAME = '${table}' ORDER BY ORDINAL_POSITION`
              : `SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = '${schema}' AND table_name = '${table}' ORDER BY ordinal_position`;
          const data = await executeSql(descSql);
          cols = data.columns ?? [];
          rows = data.rows ?? [];
          colObjs = rows.map((r) => ({
            name: r[0],
            type: r[1],
            nullable: r[2],
            default: r[3] ?? null,
          }));
        }
        // Schema describe is an internal AI operation - remove the executing
        // indicator without adding a result card. The AI gets the data via toolResult.
        const execIdx = items.findIndex((i) => i.id === execId);
        if (execIdx >= 0) items.splice(execIdx, 1);
        await scrollBottom();
        toolResult = JSON.stringify({
          table: `${schema}.${table}`,
          columns: colObjs,
        });
      } else if (call.function.name === "render_chart") {
        // Rows, whatever shape the model sent them in (a JSON string, the
        // execute_sql result, arrays): a string used to pass this check on its
        // length and crash the chart view.
        const chartSpec = { ...args, data: chartRows(args.data, args) };
        const chartId = uid();
        if (!chartSpec.data.length) {
          items.push(
            /** @type {ChatItem} */ ({
              id: chartId,
              kind: "chart",
              spec: chartSpec,
              error:
                "No data provided to render_chart. Call execute_sql first.",
            }),
          );
          toolResult = JSON.stringify({
            error:
              "No usable data. Run execute_sql first and pass its `rows` array (row objects) as `data`.",
          });
        } else {
          items.push(
            /** @type {ChatItem} */ ({
              id: chartId,
              kind: "chart",
              spec: chartSpec,
              error: null,
            }),
          );
          await scrollBottom();
          toolResult = JSON.stringify({
            success: true,
            message: "Chart rendered successfully.",
          });
        }
      } else if (call.function.name === "render_diagram") {
        const diagramType = String(args.type ?? "flowchart");
        const title = String(args.title ?? "Diagram").trim() || "Diagram";
        const code = String(args.code ?? "").trim();
        const execId = uid();
        items.push(
          /** @type {ChatItem} */ ({
            id: execId,
            kind: "executing",
            sql: title,
            op: "diagram",
          }),
        );
        await scrollBottom();
        if (!code) {
          const execIdx = items.findIndex((i) => i.id === execId);
          if (execIdx >= 0) items.splice(execIdx, 1);
          toolResult = JSON.stringify({ error: "No diagram code provided." });
        } else {
          const diagId = uid();
          const execIdx = items.findIndex((i) => i.id === execId);
          const diagItem = /** @type {ChatItem} */ ({
            id: diagId,
            kind: "diagram",
            code,
            title,
          });
          if (execIdx >= 0) items.splice(execIdx, 1, diagItem);
          else items.push(diagItem);
          await scrollBottom();
          toolResult = JSON.stringify({
            success: true,
            title,
            diagramType,
            message: "Diagram rendered. The user can save it to the Diagrams library.",
          });
        }
      } else if (call.function.name === "list_tables") {
        const tableNames = schemaContext.tables.map((t) => ({
          name: t.name,
          rowCount: t.rowCount,
        }));
        toolResult = JSON.stringify({
          schema: schemaContext.activeSchema,
          tables: tableNames,
          total: tableNames.length,
        });
      } else if (call.function.name === "get_schema") {
        const targetTable = String(args.table ?? "").trim();
        const execId = uid();
        items.push(
          /** @type {ChatItem} */ ({
            id: execId,
            kind: "executing",
            sql: targetTable || "all tables",
            op: "schema",
          }),
        );
        await scrollBottom();
        try {
          const dbType2 = schemaContext.dbType ?? "postgres";
          const isMysql = dbType2 === "mysql";
          const isSqliteFamily2 =
            dbType2 === "sqlite" || dbType2 === "d1" || dbType2 === "libsql";
          const sc = schemaContext.activeSchema.replace(/'/g, "''");

          if (isSqliteFamily2) {
            if (targetTable) {
              const tq = `"${targetTable.replace(/"/g, '""')}"`;
              const data = await executeSql(`PRAGMA table_info(${tq})`);
              const r = data.rows ?? [];
              const c = data.columns ?? [];
              const nameI = c.findIndex((x) => x.name === "name"),
                typeI = c.findIndex((x) => x.name === "type");
              const nnI = c.findIndex((x) => x.name === "notnull"),
                dfltI = c.findIndex((x) => x.name === "dflt_value");
              const cols = r.map((row) => ({
                name: row[nameI] ?? row[1],
                type: row[typeI] ?? row[2] ?? "text",
                nullable: (row[nnI] ?? row[3]) ? "NO" : "YES",
                default: row[dfltI] ?? row[4] ?? null,
              }));
              const execIdx = items.findIndex((i) => i.id === execId);
              if (execIdx >= 0) items.splice(execIdx, 1);
              toolResult = JSON.stringify({
                table: `${schemaContext.activeSchema}.${targetTable}`,
                columns: cols,
              });
            } else {
              // All tables - use already-loaded context, fall back to sqlite_master
              const tableNames = schemaContext.tables.map((t) => t.name);
              const byTable = /** @type {Record<string, unknown[]>} */ ({});
              await Promise.all(
                tableNames.map(async (tbl) => {
                  const tq = `"${tbl.replace(/"/g, '""')}"`;
                  try {
                    const data = await executeSql(`PRAGMA table_info(${tq})`);
                    const c = data.columns ?? [],
                      r = data.rows ?? [];
                    const nameI = c.findIndex((x) => x.name === "name"),
                      typeI = c.findIndex((x) => x.name === "type");
                    const nnI = c.findIndex((x) => x.name === "notnull");
                    byTable[tbl] = r.map((row) => ({
                      name: row[nameI] ?? row[1],
                      type: row[typeI] ?? row[2] ?? "text",
                      nullable: (row[nnI] ?? row[3]) ? "NO" : "YES",
                    }));
                  } catch {
                    byTable[tbl] = [];
                  }
                }),
              );
              const execIdx = items.findIndex((i) => i.id === execId);
              if (execIdx >= 0) items.splice(execIdx, 1);
              toolResult = JSON.stringify({
                schema: schemaContext.activeSchema,
                tables: byTable,
              });
            }
          } else if (targetTable) {
            const tt = targetTable.replace(/'/g, "''");
            const sql = isMysql
              ? `SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${sc}' AND TABLE_NAME = '${tt}' ORDER BY ORDINAL_POSITION`
              : `SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = '${sc}' AND table_name = '${tt}' ORDER BY ordinal_position`;
            const data = await executeSql(sql);
            const cols = (data.rows ?? []).map((r) => ({
              name: r[0],
              type: r[1],
              nullable: r[2] === "YES",
              default: r[3] ?? null,
            }));
            const execIdx = items.findIndex((i) => i.id === execId);
            if (execIdx >= 0) items.splice(execIdx, 1);
            toolResult = JSON.stringify({
              table: `${schemaContext.activeSchema}.${targetTable}`,
              columns: cols,
            });
          } else {
            const sql = isMysql
              ? `SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${sc}' ORDER BY TABLE_NAME, ORDINAL_POSITION`
              : `SELECT table_name, column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = '${sc}' ORDER BY table_name, ordinal_position`;
            const data = await executeSql(sql);
            const byTable = /** @type {Record<string, unknown[]>} */ ({});
            for (const row of data.rows ?? []) {
              const tName = String(row[0]);
              if (!byTable[tName]) byTable[tName] = [];
              byTable[tName].push({
                name: row[1],
                type: row[2],
                nullable: row[3] === "YES",
              });
            }
            const execIdx = items.findIndex((i) => i.id === execId);
            if (execIdx >= 0) items.splice(execIdx, 1);
            toolResult = JSON.stringify({
              schema: schemaContext.activeSchema,
              tables: byTable,
            });
          }
        } catch (e) {
          const execIdx = items.findIndex((i) => i.id === execId);
          if (execIdx >= 0) items.splice(execIdx, 1);
          toolResult = JSON.stringify({ error: String(e) });
        }
      } else {
        toolResult = JSON.stringify({
          error: `Unknown tool: ${call.function.name}`,
        });
      }
    } catch (e) {
      // Outer catch: JSON parse errors, unexpected exceptions - remove executing indicator silently
      items = items.filter((i) => i.kind !== "executing");
      const msg = String(e);
      const hint = classifyDbError(msg);
      const existing = failureTracker.get(callKey) ?? {
        count: 0,
        lastError: "",
      };
      failureTracker.set(callKey, {
        count: existing.count + 1,
        lastError: msg,
      });
      toolResult = JSON.stringify({
        error: msg,
        ...(hint ? { hint } : {}),
        attempt: existing.count + 1,
      });
    }
    apiHistory.push({
      role: "tool",
      tool_call_id: call.id,
      content: toolResult,
    });
  }

  /** Run SQL from a text-mode code block (user pressed Run). */
  async function runSqlBlock(/** @type {string} */ sql) {
    if (loading) return;
    if (isWriteSql(sql) && !guardWrite("run write statements from a reply")) return;
    error = "";
    if (isDestructiveSql(sql)) {
      const confirmed = await waitForConfirm(sql);
      if (!confirmed) return;
    }
    loading = true;
    // Owned like a turn, so Stop frees the composer (the statement itself
    // cannot be recalled once sent) and a later turn is not cleared by this
    // one finishing.
    const turn = ++turnSeq;
    const execId = uid();
    items.push(
      /** @type {ChatItem} */ ({
        id: execId,
        kind: "executing",
        sql,
        op: "run",
      }),
    );
    await scrollBottom();
    try {
      const data = await executeSql(sql);
      const cols = data.columns ?? [];
      const rows = data.rows ?? [];
      const sqlResId = uid();
      const resultItem = /** @type {ChatItem} */ ({
        id: sqlResId,
        kind: "result",
        sql,
        columns: cols,
        rows,
        total: data.rowCount ?? rows.length,
        error: null,
      });
      const execIdx = items.findIndex((i) => i.id === execId);
      if (execIdx >= 0) items.splice(execIdx, 1, resultItem);
      else items.push(resultItem);
      autoOpenResult(sqlResId);
      await scrollBottom();
      await persistCurrent();
    } catch (e) {
      const sqlErrId = uid();
      const errItem = /** @type {ChatItem} */ ({
        id: sqlErrId,
        kind: "result",
        sql,
        columns: [],
        rows: [],
        total: 0,
        error: String(e),
      });
      const execIdx = items.findIndex((i) => i.id === execId);
      if (execIdx >= 0) items.splice(execIdx, 1, errItem);
      else items.push(errItem);
      autoOpenResult(sqlErrId);
      await scrollBottom();
    } finally {
      if (turn === turnSeq) loading = false;
    }
  }

  /** @param {string} sql @returns {Promise<boolean>} */
  function waitForConfirm(sql) {
    return new Promise((resolve) => {
      const itemId = uid();
      items.push(
        /** @type {ChatItem} */ ({
          id: itemId,
          kind: "confirm",
          sql,
          resolve: (ok) => {
            const idx = items.findIndex((i) => i.id === itemId);
            if (idx >= 0) items.splice(idx, 1);
            resolve(ok);
          },
        }),
      );
      void scrollBottom();
    });
  }

  /** @param {string} text */
  async function copyText(text) {
    await navigator.clipboard.writeText(text).catch(() => {});
  }

  // ── Copy / quote ──────────────────────────────────────────────────────────
  /**
   * Which control last copied, so it can show a tick without every button
   * needing its own piece of state. Keyed `<itemId>:<what>`.
   */
  let copiedKey = $state("");
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let _copiedTimer;

  /** @param {string} key @param {string} text */
  async function copyWithFeedback(key, text) {
    if (!text) return;
    await copyText(text);
    copiedKey = key;
    clearTimeout(_copiedTimer);
    _copiedTimer = setTimeout(() => {
      if (copiedKey === key) copiedKey = "";
    }, 1400);
  }

  /**
   * An assistant turn as plain markdown - what a reader would expect on the
   * clipboard, fenced code and all, rather than the rendered HTML.
   * @param {import('$lib/ai.js').AssistantPart[]} parts
   */
  function assistantMarkdown(parts) {
    return (parts ?? [])
      .map((p) => {
        if (p.type === "text" || p.type === "error" || p.type === "confirm_prompt")
          return p.content;
        if (p.type === "sql") return "```sql\n" + p.content.trim() + "\n```";
        if (p.type === "mermaid") return "```mermaid\n" + p.content.trim() + "\n```";
        if (p.type === "code")
          return "```" + (p.lang || "") + "\n" + p.content.trim() + "\n```";
        return "";
      })
      .filter((s) => s.trim())
      .join("\n\n");
  }

  /**
   * Quote `text` into the composer and focus it, the way a mail client replies
   * to a passage: a `>` block plus a blank line to type into.
   * @param {string} text
   */
  function quoteToInput(text) {
    const quoted = text
      .trim()
      .split("\n")
      .map((l) => `> ${l}`)
      .join("\n");
    const prefix = inputText.trim() ? `${inputText.replace(/\s+$/, "")}\n\n` : "";
    inputText = `${prefix}${quoted}\n\n`;
    clearSelectionToolbar();
    inputRef?.focus();
    // After the value lands, so the textarea measures its new content height.
    tick().then(() => {
      resizeInput();
      if (inputRef) inputRef.selectionStart = inputRef.selectionEnd = inputText.length;
    });
  }

  /**
   * Wall-clock label under a message. Conversations saved before messages
   * carried a timestamp have none - those simply show no time.
   * @param {number | undefined} ts
   */
  function fmtMsgTime(ts) {
    if (!ts) return "";
    try {
      return new Date(ts).toLocaleTimeString([], {
        hour: "numeric",
        minute: "2-digit",
      });
    } catch {
      return "";
    }
  }

  /**
   * Drop a past prompt back into the composer, verbatim, for editing.
   * @param {string} text
   */
  function reusePrompt(text) {
    inputText = text;
    inputRef?.focus();
    tick().then(() => {
      resizeInput();
      if (inputRef) inputRef.selectionStart = inputRef.selectionEnd = inputText.length;
    });
  }

  // ── Selection toolbar ─────────────────────────────────────────────────────
  /**
   * Floating Copy / Reply bar for a text selection inside the transcript.
   *
   * Anchored in viewport coordinates (`position: fixed`) taken from the
   * selection's own rect: the transcript scrolls, and re-deriving a position
   * per scroll frame would cost more than dismissing the bar does.
   * @type {{ x: number, y: number, text: string } | null}
   */
  let selToolbar = $state(null);

  function clearSelectionToolbar() {
    selToolbar = null;
  }

  /** Read the live selection, if it lies inside the transcript. */
  function readChatSelection() {
    const sel = document.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
    const text = sel.toString().trim();
    if (!text) return null;
    const range = sel.getRangeAt(0);
    const host = msgListEl;
    if (!host) return null;
    const anchor =
      range.commonAncestorContainer instanceof Element
        ? range.commonAncestorContainer
        : range.commonAncestorContainer.parentElement;
    if (!anchor || !host.contains(anchor)) return null;
    const rect = range.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) return null;
    return { x: rect.left + rect.width / 2, y: rect.top, text };
  }

  function syncSelectionToolbar() {
    selToolbar = readChatSelection();
  }

  // A click elsewhere in the app collapses the selection without the transcript
  // ever seeing a mouseup, which would leave the bar floating over nothing.
  // Nothing reactive is read while this effect runs, so it registers once.
  $effect(() => {
    const onSelectionChange = () => {
      if (selToolbar && !readChatSelection()) clearSelectionToolbar();
    };
    document.addEventListener("selectionchange", onSelectionChange);
    return () => document.removeEventListener("selectionchange", onSelectionChange);
  });

  /**
   * Export a chat query result to a file as CSV / JSON / Markdown, reusing the
   * same formatters + native save flow as the table view.
   * @param {{ columns: any[], rows: any[][] }} item
   * @param {'csv'|'json'|'md'} format
   */
  async function exportResultAs(item, format) {
    if (!item?.rows?.length) return;
    const content =
      format === "csv"
        ? rowsToCsv(item.columns, item.rows)
        : format === "md"
          ? rowsToMarkdown(item.columns, item.rows)
          : rowsToJson(item.columns, item.rows);
    const filename = buildExportFilename("query", format);
    try {
      const saved = await saveExportFile(content, filename, format);
      if (saved)
        toast.success(`Exported ${formatCompactCount(item.rows.length)} rows`, {
          description: filename,
        });
    } catch (e) {
      toast.error("Export failed", { description: String(e) });
    }
  }

  function relativeTime(/** @type {number} */ ts) {
    const diff = (Date.now() - ts) / 1000;
    if (diff < 60) return "just now";
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return new Date(ts).toLocaleDateString();
  }

  /** @param {KeyboardEvent} e */
  function handleGlobalKey(e) {
    if (!isActive) return;
    // Escape closes fullscreen chart or diagram
    if (e.key === "Escape" && fullscreenChart) {
      e.preventDefault();
      closeChartFullscreen();
      return;
    }
    if (e.key === "Escape" && fullscreenDiagramCode) {
      e.preventDefault();
      closeDiagramFullscreen();
      return;
    }
    // The Stop button advertises "Stop (Esc)" - honor it after the fullscreen closers.
    if (e.key === "Escape" && loading) {
      e.preventDefault();
      stop();
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || !e.shiftKey) return;
    const key = e.key.toLowerCase();
    if (key === "b") {
      e.preventDefault();
      toggleAiSidebar();
    } else if (key === "t") {
      e.preventDefault();
      void newConversation();
    }
  }

  // Reload conversation list when user connects to a different database
  let prevConnectionId = "";
  $effect(() => {
    const id = connectionId;
    if (id === prevConnectionId) return;
    prevConnectionId = id;
    abortCurrentRequest();
    activeConvId = null;
    items = [];
    apiHistory = [];
    rawApiHistory = [];
    fetchedSchemas = {}; // drop the previous DB's cached schema columns (avoids cross-connection growth)
    sampledRows = {}; // keys are `schema.table` - a colliding key would leak the old DB's rows into new prompts
    error = "";
    openTabIds = loadOpenTabs(id); // reset + reload the open-tab set for the new connection
    loadConvList();
  });

  /**
   * Ctrl+B hides and shows the chat list while the chat is on screen. The app
   * binds it to its own sidebar, so this listens in the capture phase and
   * keeps the key from reaching that binding.
   * @param {KeyboardEvent} e
   */
  function onChatListKey(e) {
    // In the window's AI mode the app's own ⌘B handler toggles this list
    // (StudioShell → toggleChats). Handling it here too toggled it twice per
    // press - shown then hidden again - so ⌘B looked dead.
    if (externalToggle) return;
    if (!(isActive || mode === "full")) return;
    if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey || e.key.toLowerCase() !== "b") return;
    e.preventDefault();
    e.stopImmediatePropagation();
    toggleAiSidebar();
  }

  onMount(() => {
    void tick().then(() => inputRef?.focus());
    document.addEventListener("keydown", handleGlobalKey);
    window.addEventListener("keydown", onChatListKey, true);
    // Pre-warm schema cache so the first message doesn't pay the fetch cost
    if (schemaContext.tables?.length) void ensureFullSchemaCache();
  });

  onDestroy(() => {
    document.removeEventListener("keydown", handleGlobalKey);
    window.removeEventListener("keydown", onChatListKey, true);
    stopAgentSession();
    // Cancel any pending rAF handles to avoid callbacks on destroyed DOM
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    if (_streamTimer !== null) {
      clearTimeout(_streamTimer);
      _streamTimer = null;
    }
    if (historyTimer) {
      clearTimeout(historyTimer);
      historyTimer = null;
    }
  });
  const isMacChrome = typeof document !== 'undefined' && document.documentElement.dataset.os === 'macos'
</script>

<!-- Single, unified agent loading indicator (icon + phase label + dots). Used for
     both the in-flow "thinking" placeholder and the bottom "working" state so they
     are pixel-identical and never visually drift or double up. -->
{#snippet generatingCard(/** @type {'chart' | 'diagram'} */ what)}
  <!-- A chart or diagram on its way: the card it will land in, with a sweep of
       light and a live sketch of the thing being made. Every animation is a
       transform or opacity, so it runs on the compositor while the reply
       streams and the main thread is busy. -->
  <div class="relative overflow-hidden rounded-lg border border-border/60 bg-muted/10" role="status" aria-live="polite">
    <div class="flex h-8 items-center gap-2 border-b border-border/40 bg-muted/20 px-3">
      {#if what === "chart"}<BarChart3 class="size-3.5 shrink-0 text-primary" />{:else}<GitBranch class="size-3.5 shrink-0 text-primary" />{/if}
      <span class="text-ui-xs font-medium text-foreground/80">{what === "chart" ? "Generating chart" : "Generating diagram"}</span>
      <span class="ai-gen-dots flex items-center gap-0.5 text-muted-foreground" aria-hidden="true"><i></i><i></i><i></i></span>
    </div>
    <div class="relative flex h-44 items-end justify-center gap-2 px-8 pb-6 pt-6">
      {#if what === "chart"}
        {#each [42, 68, 54, 86, 61, 74, 48] as h, k (k)}
          <span class="ai-gen-bar w-6 rounded-t-sm bg-primary/25" style="height: {h}%; animation-delay: {k * 90}ms"></span>
        {/each}
      {:else}
        <svg class="absolute inset-0 m-auto h-32 w-64 text-primary" viewBox="0 0 256 128" fill="none" aria-hidden="true">
          <path class="ai-gen-line" d="M78 34 H128 V94 H178" stroke="currentColor" stroke-opacity="0.35" stroke-width="1.5" stroke-dasharray="4 4" />
          <path class="ai-gen-line" d="M78 34 H128 V34 H178" stroke="currentColor" stroke-opacity="0.35" stroke-width="1.5" stroke-dasharray="4 4" style="animation-delay: 300ms" />
          <rect class="ai-gen-node" x="18" y="18" width="60" height="32" rx="6" fill="currentColor" fill-opacity="0.16" stroke="currentColor" stroke-opacity="0.4" />
          <rect class="ai-gen-node" x="178" y="18" width="60" height="32" rx="6" fill="currentColor" fill-opacity="0.16" stroke="currentColor" stroke-opacity="0.4" style="animation-delay: 250ms" />
          <rect class="ai-gen-node" x="178" y="78" width="60" height="32" rx="6" fill="currentColor" fill-opacity="0.16" stroke="currentColor" stroke-opacity="0.4" style="animation-delay: 500ms" />
        </svg>
      {/if}
      <span class="ai-gen-sweep pointer-events-none absolute inset-0" aria-hidden="true"></span>
    </div>
  </div>
{/snippet}

{#snippet agentIndicator()}
  <div class="flex h-7 items-center gap-2 px-3.5" role="status" aria-live="polite">
    <!-- A quiet spinner and the label, as T3 shows work in progress. The tinted
         badge that sat here was a third signal competing with the two. -->
    <LoaderCircle class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
    <!-- The label's own shimmer carries the "working" signal; a trailing row of
         bouncing dots on top of it was a second animation saying the same thing. -->
    <span
      class="agent-think-label text-ui-xs text-muted-foreground transition-opacity duration-200 {thinkingVisible ? 'opacity-100' : 'opacity-0'}"
      >{loadingText}</span
    >
  </div>
{/snippet}

<!-- The plan a multi-step turn opens with, ticked as its steps finish. Status
     reads from the icon and a screen-reader word; color is never the only cue. -->
{#snippet planChecklist(/** @type {string[]} */ plan, /** @type {string[]} */ statuses)}
  <ol class="flex flex-col gap-1 py-1" aria-label="Plan">
    {#each plan as line, i (i)}
      {@const st = statuses[i] ?? "pending"}
      <li class="flex min-w-0 items-start gap-2 text-ui-sm">
        <span class="flex h-5 shrink-0 items-center">
          {#if st === "done"}
            <CircleCheck class="size-3.5 shrink-0 text-primary" />
          {:else if st === "error"}
            <CircleX class="size-3.5 shrink-0 text-destructive" />
          {:else if st === "running"}
            <span class="flex size-3.5 shrink-0 items-center justify-center" aria-hidden="true"><span class="size-3 animate-spin rounded-full border-[1.5px] border-muted-foreground/25 border-t-muted-foreground will-change-transform"></span></span>
          {:else}
            <Circle class="size-3.5 shrink-0 text-muted-foreground" />
          {/if}
        </span>
        <span class={cn("min-w-0 break-words leading-5", st === "done" || st === "pending" ? "text-muted-foreground" : "text-foreground")}>{line}</span>
        <span class="sr-only">({st === "error" ? "failed" : st})</span>
      </li>
    {/each}
  </ol>
{/snippet}

<!-- One step of a turn: status icon, what it does, how long it took. A query
     step opens to its SQL and result; a failure says why, in red, without a click. -->
{#snippet stepRow(/** @type {WorkStep} */ step)}
  {@const isQuery = step.op === "query"}
  {@const meta = isQuery ? { label: sqlStepLabel(step.detail), detail: "" } : stepLabel(step)}
  {@const dur = step.startedAt && step.endedAt ? formatStepDuration(step.endedAt - step.startedAt) : ""}
  {@const expandable = isQuery && !!step.detail.trim()}
  {@const open = openSteps.has(step.id)}
  {#snippet rowInner()}
    {#if step.status === "running"}
      <span class="flex size-3.5 shrink-0 items-center justify-center" aria-hidden="true"><span class="size-3 animate-spin rounded-full border-[1.5px] border-muted-foreground/25 border-t-muted-foreground will-change-transform"></span></span>
    {:else if step.status === "error"}
      <CircleX class="size-3.5 shrink-0 text-destructive" />
    {:else}
      <CircleCheck class="size-3.5 shrink-0 text-muted-foreground" />
    {/if}
    <span class="min-w-0 truncate text-ui-xs text-foreground">{meta.label}</span>
    <span class="sr-only">({step.status === "error" ? "failed" : step.status === "running" ? "running" : "done"})</span>
    {#if meta.detail}
      <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground">{meta.detail}</span>
    {:else}
      <span class="flex-1"></span>
    {/if}
    {#if dur}
      <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground">{dur}</span>
    {/if}
    {#if expandable}
      <ChevronRight class={cn("size-3.5 shrink-0 text-muted-foreground", open && "rotate-90")} />
    {/if}
  {/snippet}
  <div class="flex min-w-0 flex-col gap-1">
    {#if expandable}
      <button
        type="button"
        class="-mx-1.5 flex h-7 min-w-0 items-center gap-2 rounded-md px-1.5 text-left transition-colors hover:bg-accent"
        aria-expanded={open}
        onclick={() => toggleStep(step.id)}
      >{@render rowInner()}</button>
    {:else}
      <div class="flex h-7 min-w-0 items-center gap-2">{@render rowInner()}</div>
    {/if}
    {#if step.error}
      <p class="pl-5.5 text-ui-2xs leading-relaxed break-words whitespace-pre-wrap text-destructive">{cleanToolError(step.error)}</p>
    {/if}
    {#if expandable && open}
      <div class="flex flex-col gap-1 pl-5.5">
        <CollapsibleCode text={step.detail.trim()} max={160} class="rounded-md bg-muted/30 px-2.5 py-1.5 font-mono text-ui-2xs whitespace-pre-wrap break-words text-muted-foreground" />
        {#if step.status === "done" && (step.message || step.affected != null)}
          <p class="font-mono text-ui-2xs text-muted-foreground">{step.message || `${step.affected} row(s)`}</p>
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

<!-- Per-message footer: time + copy (+ quote for assistant turns).
     Hidden until the message is hovered or a control inside it takes focus, so a
     long transcript stays quiet - but always laid out, so revealing it can't
     shift the message above. -->
{#snippet msgActions(key, text, ts, align, canQuote)}
  <!-- Deliberately smaller than the app's default control size: this row sits
       under body copy as a footnote, so size-7 buttons and size-3.5 icons read
       as heavier than the message they belong to. -->
  <!-- The time hugs the message's edge (the reply's first letter, the bubble's
       end) and the buttons sit inward of it, so the row mirrors for your own
       turns. h-6 holds the 24px buttons: a 20px row let them spill 2px into
       the message above and below. -->
  <div
    class="flex h-6 items-center gap-0.5 opacity-0 transition-opacity duration-100 group-hover/msg:opacity-100 focus-within:opacity-100 {align ===
    'end'
      ? 'flex-row-reverse'
      : ''}"
  >
    {#if ts}
      <span class="text-ui-3xs tabular-nums text-muted-foreground {align === 'end' ? 'ms-1.5' : 'me-1.5'}"
        >{fmtMsgTime(ts)}</span
      >
    {/if}
    <button
      type="button"
      class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      title="Copy message"
      aria-label="Copy message"
      onclick={() => void copyWithFeedback(`${key}:msg`, text)}
    >
      {#if copiedKey === `${key}:msg`}
        <Check class="size-3 shrink-0 text-success" strokeWidth={2.5} />
      {:else}
        <Copy class="size-3 shrink-0" />
      {/if}
    </button>
    {#if canQuote}
      <button
        type="button"
        class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="Reply to this message"
        aria-label="Reply to this message"
        onclick={() => quoteToInput(text)}
      >
        <Reply class="size-3 shrink-0" />
      </button>
    {:else}
      <!-- Your own turn: put it back in the composer to ask it again with a
           tweak, which is what re-running a prompt usually means. -->
      <button
        type="button"
        class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title="Edit and ask again"
        aria-label="Edit and ask again"
        onclick={() => reusePrompt(text)}
      >
        <Pencil class="size-3 shrink-0" />
      </button>
    {/if}
  </div>
{/snippet}

<div class="flex h-full min-h-0 overflow-hidden bg-background">
  <!-- ── Conversation sidebar ───────────────────────────────────────── -->
  {#if sidebarVisible && !listInSidebar}
    <aside
      class="flex shrink-0 flex-col border-r border-border/50 bg-sidebar"
      style="width: calc({sidebarWidth}px * var(--app-scale, 1))"
    >
      <!-- Sidebar header: collapse + new chat. In the window's AI mode this is
           the top of a full-height column (T3): the traffic lights sit in it. -->
      <div
        class={cn("flex shrink-0 items-center gap-1", externalToggle && isMacChrome ? "h-[52px] pr-2 pl-[92px]" : "border-b border-border/50 px-2 py-2")}
        data-tauri-drag-region={externalToggle ? '' : undefined}
      >
        {#if !externalToggle}
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Hide chats ({modKey}B)"
          onclick={toggleAiSidebar}
        >
          <PanelLeft class="size-3.5 shrink-0" />
        </button>
        {/if}
        <span
          class="flex-1 px-1 text-ui-xs font-medium text-muted-foreground"
          >History</span
        >
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="New chat ({newChatShortcut})"
          onclick={() => void newConversation()}
        >
          <Plus class="size-3.5 shrink-0" />
        </button>
      </div>

      <!-- List toolbar: count and Select, or the selection's actions. -->
      {#if convList.length > 0}
        <div class="flex h-8 shrink-0 items-center gap-1 px-3 text-ui-2xs text-muted-foreground">
          {#if selecting}
            <span class="tabular-nums text-foreground/80">{selectedConvs.size} selected</span>
            <button
              type="button"
              class="rounded-md px-1.5 py-0.5 transition-colors hover:bg-accent hover:text-foreground"
              onclick={() => (selectedConvs = selectedConvs.size === convList.length ? new Set() : new Set(convList.map((c) => c.id)))}
            >{selectedConvs.size === convList.length ? "None" : "All"}</button>
            <span class="flex-1"></span>
            <button
              type="button"
              disabled={selectedConvs.size === 0}
              class="rounded-md px-1.5 py-0.5 text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-40"
              onclick={() => void removeConversations([...selectedConvs])}
            >Delete</button>
            <button
              type="button"
              class="rounded-md px-1.5 py-0.5 transition-colors hover:bg-accent hover:text-foreground"
              onclick={endSelecting}
            >Done</button>
          {:else}
            <span class="tabular-nums">{convList.length} {convList.length === 1 ? "chat" : "chats"}</span>
            <span class="flex-1"></span>
            <button
              type="button"
              class="rounded-md px-1.5 py-0.5 transition-colors hover:bg-accent hover:text-foreground"
              onclick={() => (selecting = true)}
            >Select</button>
            <button
              type="button"
              class={cn(
                "rounded-md px-1.5 py-0.5 transition-colors",
                confirmClearAll ? "bg-destructive/10 text-destructive" : "hover:bg-accent hover:text-foreground",
              )}
              onclick={() => {
                if (!confirmClearAll) {
                  confirmClearAll = true;
                  setTimeout(() => (confirmClearAll = false), 3000);
                  return;
                }
                void removeConversations(convList.map((c) => c.id));
              }}
              title={confirmClearAll ? "Click again to delete every chat" : "Delete every chat"}
            >{confirmClearAll ? "Confirm" : "Clear all"}</button>
          {/if}
        </div>
      {/if}

      <!-- Conversation list -->
      <div class="app-scroll flex min-h-0 flex-1 flex-col gap-px overflow-y-auto px-1.5 py-1">
        {#if isDraftChat}
          <button
            type="button"
            class="relative flex h-9 w-full min-w-0 items-center gap-2 rounded-md bg-sidebar-accent px-2.5 text-left text-foreground"
          >
            <span class="min-w-0 flex-1 truncate text-ui-xs font-medium">New chat</span>
            <span class="shrink-0 text-ui-3xs text-muted-foreground">Draft</span>
          </button>
        {/if}

        {#each convList as conv (conv.id)}
          {@const isActive = activeConvId === conv.id}
          {@const isRenaming = renamingConvId === conv.id}
          <div class="group/conv relative">
            {#if isRenaming}
              <div class="flex h-9 items-center gap-1 px-1">
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  autofocus
                  type="text"
                  bind:value={renamingTitle}
                  class="min-w-0 flex-1 rounded-lg border-2 border-border bg-background px-1.5 py-0.5 font-mono text-ui-2xs text-foreground outline-none focus:border-ring/55 focus:ring-2 focus:ring-ring/15"
                  onkeydown={(e) => {
                    if (e.key === "Enter") void commitRename();
                    if (e.key === "Escape") cancelRename();
                  }}
                  onblur={() => void commitRename()}
                />
              </div>
            {:else}
              {@const isSel = selectedConvs.has(conv.id)}
              <button
                type="button"
                class={cn(
                  // A rounded inset row: title left, time right, the way the
                  // table sidebar lists tables. Hover actions replace the time
                  // in place instead of covering the title.
                  // Focus is a soft fill like hover, never a border or bar.
                  "relative flex h-9 w-full min-w-0 items-center gap-2 rounded-md px-2.5 text-left transition-colors outline-none focus-visible:bg-sidebar-accent/80 focus-visible:text-foreground",
                  isSel
                    ? "bg-primary/10 text-foreground"
                    : isActive && !selecting
                      ? "bg-sidebar-accent text-foreground"
                      : "text-muted-foreground hover:bg-sidebar-accent/60 hover:text-foreground",
                )}
                aria-pressed={selecting ? isSel : undefined}
                onclick={(e) => {
                  // Shift: everything from the anchor (last row clicked, or the
                  // open chat) to this one. ⌘/Ctrl or select mode: toggle one.
                  if (e.shiftKey) {
                    const ids = convList.map((c) => c.id);
                    const from = ids.indexOf(convAnchor ?? activeConvId ?? conv.id);
                    const to = ids.indexOf(conv.id);
                    const [lo, hi] = from < 0 ? [to, to] : from < to ? [from, to] : [to, from];
                    const next = new Set(selectedConvs);
                    for (let k = lo; k <= hi; k++) next.add(ids[k]);
                    selectedConvs = next;
                    selecting = true;
                    convAnchor = conv.id;
                    return;
                  }
                  if (selecting || e.metaKey || e.ctrlKey) {
                    if (!selecting) selecting = true;
                    toggleConvSelected(conv.id);
                    convAnchor = conv.id;
                    return;
                  }
                  convAnchor = conv.id;
                  void selectConversation(conv.id);
                }}
                ondblclick={() => { if (!selecting) startRename(conv.id, conv.title); }}
                oncontextmenu={(e) => showContextMenu(conv.id, e)}
              >
                {#if selecting}
                  <span class={cn("flex size-3.5 shrink-0 items-center justify-center rounded-sm border", isSel ? "border-primary bg-primary text-primary-foreground" : "border-muted-foreground/40")}>
                    {#if isSel}<Check class="size-2.5 shrink-0" strokeWidth={3} />{/if}
                  </span>
                {/if}
                <span class="min-w-0 flex-1 truncate text-ui-xs font-medium leading-snug">{displayTitle(conv)}</span>
                <span class={cn("shrink-0 text-ui-3xs tabular-nums text-muted-foreground", !selecting && "group-hover/conv:invisible")}>{relativeTime(conv.updatedAt)}</span>
              </button>
              {#if !selecting}
              <div
                class="absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-0.5 opacity-0 transition-opacity group-hover/conv:opacity-100"
              >
                <button
                  type="button"
                  class="hit-area flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  title="Rename"
                  onclick={(e) => {
                    e.stopPropagation();
                    startRename(conv.id, conv.title);
                  }}
                >
                  <Pencil class="size-3 shrink-0" />
                </button>
                <button
                  type="button"
                  class="hit-area flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-destructive"
                  title="Delete"
                  onclick={(e) => {
                    e.stopPropagation();
                    void removeConversation(conv.id);
                  }}
                >
                  <Trash2 class="size-3 shrink-0" />
                </button>
              </div>
              {/if}
            {/if}
          </div>
        {/each}

        {#if convList.length === 0 && !isDraftChat}
          <div
            class="flex flex-col items-center gap-1.5 px-4 py-10 text-center"
          >
            <p class="text-ui-2xs text-muted-foreground">
              No conversations yet
            </p>
            <p class="text-ui-3xs text-muted-foreground">
              Chats save automatically
            </p>
          </div>
        {/if}
      </div>
      {#if onexit}
        <!-- The table sidebar's footer, so both left columns end the same way:
             app-wide controls as plain icons, and the one round action on the
             right - here, back to the data. -->
        <div class="flex h-12 shrink-0 items-center gap-1 px-3">
          {#if onopensettings}
            <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title="Settings" aria-label="Settings" onclick={onopensettings}><Icon name="settings" class="size-4 shrink-0" /></button>
          {/if}
          <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title={$isCurrentThemeDark ? "Light theme" : "Dark theme"} aria-label="Toggle theme" onclick={() => toggleLightDark()}><Icon name={$isCurrentThemeDark ? "sun" : "moon"} class="size-4 shrink-0" /></button>
          {#if onopenshortcuts}
            <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title="Keyboard shortcuts" aria-label="Keyboard shortcuts" onclick={onopenshortcuts}><Icon name="keyboard" class="size-4 shrink-0" /></button>
          {/if}
          <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title="Hide chats ({modKey}B)" aria-label="Hide chats" onclick={toggleAiSidebar}><PanelLeft class="size-4 shrink-0" /></button>
          <button
            type="button"
            class="ml-auto inline-flex size-8 items-center justify-center rounded-full text-muted-foreground outline-none hover:bg-sidebar-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            title="Exit AI mode ({modKey}⇧E)"
            aria-label="Exit AI mode"
            onclick={onexit}
          ><Minimize2 class="size-4 shrink-0" /></button>
        </div>
      {/if}
    </aside>
    <!-- Drag, or arrow keys when focused, to widen the chat list; double-click resets. -->
    <div class="-ml-1 flex" ondblclick={() => { sidebarWidth = externalToggle ? NAV_SIDEBAR_MIN : SIDEBAR_DEFAULT_W; saveSidebarWidth(); }} role="presentation">
      <ResizeHandle
        axis="x"
        edge="end"
        onresizestart={() => { sidebarResizeStart = sidebarWidth; sidebarResizeScale = getAppScale(); }}
        onresize={(dx) => { const w = sidebarResizeStart + dx / sidebarResizeScale; sidebarWidth = externalToggle ? clampNavSidebarWidth(w) : Math.min(SIDEBAR_MAX_W, Math.max(SIDEBAR_MIN_W, w)); }}
        onresizeend={saveSidebarWidth}
      />
    </div>
  {/if}

{#snippet composerCard()}
        <!-- Soft-cornered surface rather than a hard-edged box: the field should
             read as one calm object floating over the transcript, and only pick up
             weight (border, lift) once it has focus. -->
        <div
          class={cn(
            // One clear edge at rest, a focus ring when typing: the field shows
            // where it is without a heavy box, and focus is unmistakable.
            "rounded-2xl border border-border bg-sidebar shadow-[0_1px_2px_rgb(0_0_0/0.05)] transition-[border-color,box-shadow] duration-150 focus-within:border-ring/50 focus-within:shadow-[0_0_0_3px_color-mix(in_oklch,var(--ring)_14%,transparent)]",
            hasPendingConfirm && "opacity-50",
          )}
        >
          {#if queued.length}
            <!-- Waiting to be sent, in order; each goes out as a turn ends. -->
            <ul class="flex flex-col gap-1 border-b border-border/40 px-2 pt-2 pb-1.5" aria-label="Queued messages">
              {#each queued as q, qi (q.id)}
                <li class="ai-enter group/q flex min-w-0 items-center gap-2 rounded-lg bg-muted/40 py-1 pr-1 pl-2.5">
                  <CornerDownRight class="size-3.5 shrink-0 text-muted-foreground" />
                  <span class="min-w-0 flex-1 truncate text-ui-xs text-foreground/90" title={q.text}>{q.text}</span>
                  <span class="shrink-0 text-ui-3xs text-muted-foreground">{qi === 0 ? (loading ? "Next" : "Queued") : `#${qi + 1}`}</span>
                  <button
                    type="button"
                    class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    title="Remove from queue"
                    aria-label="Remove from queue"
                    onclick={() => unqueue(q.id)}
                  >
                    <X class="size-3.5 shrink-0" />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
          <!-- Textarea row. Font size comes from the same var as the transcript
               (--ai-chat-font-size, Settings → Appearance), so what you type is
               set at the size you'll read it back at. -->
          <textarea
            bind:this={inputRef}
            class="ai-composer-input no-focus-ring block w-full resize-none bg-transparent px-5 pt-4 pb-3 text-foreground outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed"
            style="height:auto;min-height:60px;max-height:200px;overflow-y:auto;font-family:inherit"
            placeholder={hasPendingConfirm
              ? "Confirm or cancel the operation above…"
              : items.length === 0 ? "Ask about your data…" : "Ask a follow-up…"}
            rows={1}
            value={inputText}
            oninput={(e) => {
              inputText = /** @type {HTMLTextAreaElement} */ (e.target).value;
              resizeInput();
              pushHistory(inputText);
            }}
            onkeydown={handleKeydown}
            onfocus={() => {
              inputFocused = true;
              prewarmAgent();
            }}
            onblur={() => (inputFocused = false)}
            disabled={hasPendingConfirm}
          ></textarea>

          <!-- Bottom toolbar row: the controls that change what the next turn
               does sit together on the left, separated by hairlines; the send
               affordance stays alone on the right. -->
          <div class="flex min-w-0 items-center gap-1.5 px-3 pb-3">
            {#if contextStats.messages > 0}
              <button
                type="button"
                class="inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground select-none"
                onclick={() => {
                  settingsOpen = true;
                  settingsTab = "context";
                }}
                title="Context usage - click for a breakdown"
              >
                <BarChart2 class="size-3 shrink-0" />
                <span class="tabular-nums"
                  >{tokEst(contextStats.totalChars)}</span
                >
                {#if contextStats.pct >= 70}
                  <span
                    class={contextStats.pct >= 90
                      ? "text-destructive"
                      : "text-warning"}>· {contextStats.pct}%</span
                  >
                {/if}
              </button>
            {/if}

            <div class="min-w-2 flex-1"></div>

            <!-- Shortcut hint only while the empty composer holds focus: it is
                 guidance for someone about to type, not a permanent label. -->
            {#if loading && inputText.trim()}
              <span class="hidden shrink-0 text-ui-2xs text-muted-foreground sm:block">↵ to queue</span>
            {:else if hasPendingConfirm}
              <span class="hidden shrink-0 text-ui-2xs text-muted-foreground sm:block">Confirm or cancel above</span>
            {/if}

            <AiModelPicker onopenSettings={onopenmodelsettings} />

            <!-- Send / Stop. One control, one meaning: the spinner became the halo
                 around Stop instead of a second indicator sitting beside it, and the
                 idle Send button stays weightless until there is something to send. -->
            {#if loading}
              <div class="relative flex shrink-0 items-center justify-center">
                <span
                  class="pointer-events-none absolute -inset-[3px] animate-spin rounded-full border-[1.5px] border-transparent border-t-destructive/70"
                  aria-hidden="true"
                ></span>
                <button
                  type="button"
                  class="flex size-8 items-center justify-center rounded-full bg-destructive/90 text-destructive-foreground transition-colors hover:bg-destructive active:scale-95 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                  onclick={stop}
                  aria-label="Stop"
                  title="Stop (Esc)"
                >
                  <Square class="size-2 shrink-0 fill-current" />
                </button>
              </div>
            {:else}
              {@const canSend = !!inputText.trim() && !hasPendingConfirm}
              <button
                type="button"
                class={cn(
                  // T3's send: a round button, grey until there is something to send.
                  "flex size-8 shrink-0 items-center justify-center rounded-full transition-[background-color,opacity,transform] duration-150 active:scale-95 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
                  canSend
                    ? "bg-foreground text-background hover:opacity-90"
                    : "bg-sidebar-accent text-muted-foreground cursor-not-allowed",
                )}
                disabled={!canSend}
                onclick={() => void send()}
                aria-label="Send"
                title="Send (↵)"
              >
                <ArrowUp class="size-4 shrink-0" strokeWidth={2.25} />
              </button>
            {/if}
          </div>
        </div>
        <!-- One quiet row under the composer: what the chat is looking at on
             the left, what the agent may do on the right. -->
        <div class="flex min-w-0 items-center gap-3 px-4 pt-2.5 text-ui-xs text-muted-foreground">
          {#if schemaContext.activeSchema}
            <span class="flex min-w-0 items-center gap-1.5 truncate" title="Schema in context">
              <Database class="size-3.5 shrink-0" />
              <span class="truncate">{schemaContext.activeTable ? `${schemaContext.activeSchema}.${schemaContext.activeTable}` : schemaContext.activeSchema}</span>
            </span>
          {/if}
          <span class="flex-1"></span>
          {#if profileAgent($aiSettings)}
            <AgentComposerChips agent={profileAgent($aiSettings) ?? ""} part="access" />
          {:else if $readOnlyMode}
            <span class="flex shrink-0 items-center gap-1.5"><Lock class="size-3.5 shrink-0" />Read only</span>
          {/if}
        </div>
{/snippet}

  <!-- ── Main chat area ─────────────────────────────────────────────── -->
  <div class="relative flex min-h-0 min-w-0 flex-1 flex-col">
    {#if externalToggle && isMacChrome}
      <!-- The window's top row over the conversation: drags the window, and
           leaves the lights room when the chat list is hidden. -->
      <div class="h-[52px] shrink-0" data-tauri-drag-region></div>
    {/if}
    <!-- No header row: its four buttons cost a 48px band above the transcript.
         In full mode they float over the transcript's top corners; docked, they
         sit at the ends of the conversation tab bar that is there anyway. -->
    {#snippet chatListButton()}
      {#if !externalToggle && !listInSidebar}
      <button
        type="button"
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title={sidebarVisible ? `Hide chats (${modKey}B)` : `Show chats (${modKey}B)`}
        aria-label={sidebarVisible ? "Hide chats" : "Show chats"}
        onclick={toggleAiSidebar}
      >
        <PanelLeft class="size-3.5 shrink-0" />
      </button>
      {/if}
    {/snippet}
    {#snippet chromeRight()}
      <button
        type="button"
        class={cn(
          "inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
          settingsOpen && "bg-accent text-foreground",
        )}
        title="Settings ({modKey}⇧,)"
        aria-label="Settings"
        onclick={() => {
          settingsOpen = !settingsOpen;
          settingsTab = "model";
        }}
      >
        <Settings2 class="size-3.5 shrink-0" />
      </button>
      {#if onexit}
        <button
          type="button"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Exit AI mode ({modKey}⇧E)"
          aria-label="Exit AI mode"
          onclick={onexit}
        >
          <!-- Not an X: the window's close sits right above this corner, and two
               stacked X's read as the same button. This one leaves AI mode. -->
          <Minimize2 class="size-3.5 shrink-0" />
        </button>
      {/if}
    {/snippet}

    {#if !showTabBar}
      <div class={cn("pointer-events-none absolute inset-x-0 top-0 z-20 flex items-start justify-between p-2", externalToggle && isMacChrome && "pt-2.5", externalToggle && isMacChrome && !sidebarVisible && "pl-[92px]")}>
        <div class="pointer-events-auto flex items-center gap-0.5 rounded-lg bg-background/90 p-0.5">
          {@render chatListButton()}
          {#if !sidebarVisible}
            <button
              type="button"
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              title="New chat ({newChatShortcut})"
              aria-label="New chat"
              onclick={() => void newConversation()}
            >
              <Plus class="size-3.5 shrink-0" />
            </button>
          {/if}
        </div>
        <div class="pointer-events-auto flex items-center gap-0.5 rounded-lg bg-background/90 p-0.5">
          {@render chromeRight()}
        </div>
      </div>
      <!-- Text scrolling up fades out under the buttons instead of running into them. -->
      <div class="pointer-events-none absolute inset-x-0 top-0 z-10 h-10 bg-gradient-to-b from-background to-transparent" aria-hidden="true"></div>
    {/if}

    <!-- ── Conversation tab bar ─────────────────────────────────────────
         Not in full AI mode: there the conversation sidebar already lists every
         chat and carries its own New-chat button, so the tab strip was a second
         switcher for the same thing, eating a row of vertical space above the
         messages. The docked panel keeps it (it is narrow enough that the
         sidebar is usually collapsed there). -->
    {#if showTabBar}
      <div class="flex shrink-0 items-center gap-1 border-b border-border bg-panel px-1.5 py-1">
      {@render chatListButton()}
      <div
        class="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {#if !activeConvId}
          <!-- Unsaved draft tab: always active, never closeable -->
          <div
            class="flex shrink-0 items-center rounded-md bg-accent px-2 py-1 font-mono text-ui-xs text-foreground"
            title="New chat"
          >
            <span class="max-w-[9rem] truncate">New chat</span>
          </div>
        {/if}
        {#each openTabs as tab (tab.id)}
          {@const isActive = activeConvId === tab.id}
          <div
            class={cn(
              "group/tab flex shrink-0 items-center gap-1 rounded-md py-1 pl-2 pr-1 font-mono text-ui-xs transition-colors",
              isActive
                ? "bg-accent text-foreground"
                : "text-muted-foreground hover:bg-accent/40 hover:text-foreground",
            )}
          >
            <button
              type="button"
              class="max-w-[9rem] truncate text-left"
              title={displayTitle(tab)}
              onclick={() => void selectConversation(tab.id)}
            >
              {displayTitle(tab)}
            </button>
            <button
              type="button"
              class="hit-area flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[opacity,color] hover:text-foreground group-hover/tab:opacity-100"
              title="Close tab"
              onclick={(e) => {
                e.stopPropagation();
                closeTab(tab.id);
              }}
            >
              <X class="size-3 shrink-0" />
            </button>
          </div>
        {/each}
        <button
          type="button"
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="New chat ({newChatShortcut})"
          onclick={() => void newConversation()}
        >
          <Plus class="size-3.5 shrink-0" />
        </button>
      </div>
      {@render chromeRight()}
      </div>
    {/if}

    <!-- Messages -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      bind:this={scrollEl}
      onscroll={() => {
        // The bar is anchored to a viewport rect, so scrolling would leave it
        // stranded away from its own text. Dismiss instead of re-measuring.
        if (selToolbar) clearSelectionToolbar();
      }}
      onmousedown={clearSelectionToolbar}
      onmouseup={syncSelectionToolbar}
      onkeyup={syncSelectionToolbar}
      class="app-scroll ai-transcript min-h-0 flex-1 overflow-y-auto relative [overflow-anchor:none] overscroll-contain [contain:layout_paint] [-webkit-overflow-scrolling:touch]"
      role="region"
      aria-label="Chat messages"
    >
      <!-- Conversation column matches the composer width (max-w-3xl): a wider
           measure gave 200+ character lines on big windows and left the input
           visually detached from the messages above it. Wide artifacts inside
           messages (result grids, diagrams) scroll within their own cards. -->
      <!-- Padding lives on the outer box and the measure on the inner one -
           exactly as the composer is built - so the transcript column and the
           composer card share one frame. Message rows then carry the same
           px-3.5 as the textarea, and a reply lines up with what you typed. -->
      <div
        class={mode === "full"
          ? items.length === 0
            ? "px-6 h-full"
            : "px-6"
          : items.length === 0
            ? "px-3 h-full"
            : "px-3 py-3"}
      >
      <div
        class={mode === "full"
          ? items.length === 0
            ? "mx-auto w-full max-w-3xl h-full"
            : "mx-auto w-full max-w-3xl"
          : items.length === 0
            ? "h-full"
            : ""}
      >
        {#if items.length === 0}
          <!-- ── Landing, T3's new-thread screen: one centred column, a light
               headline naming the database, one muted line, the composer right
               under it, and at most four quiet suggestions below. The side
               panel gets the same pattern, scaled down. -->
          {@const full = mode === "full"}
          <div class={cn("mx-auto flex h-full w-full flex-col justify-center", full ? "max-w-[46rem] gap-7 py-12" : "gap-4 py-6")}>
            <div class="flex flex-col items-center gap-1.5 text-center">
              <h1 class={cn("font-light tracking-[-0.02em] text-foreground", full ? "text-ui-3xl" : "text-ui-xl")}>
                What do you want to know{#if dbName}{" "}about <span class="font-normal">{dbName}</span>{/if}?
              </h1>
              {#if schemaContext.activeTable || schemaContext.tables?.length}
                <p class="truncate text-ui-sm text-muted-foreground">
                  {#if schemaContext.activeTable}
                    {schemaContext.activeSchema}.{schemaContext.activeTable}
                  {:else}
                    {schemaContext.activeSchema} · {schemaContext.tables.length} tables
                  {/if}
                </p>
              {/if}
            </div>

            <div>{@render composerCard()}</div>

            {#if suggestions.length > 0 && !full}
              <div class="flex flex-col gap-0.5">
                {#each suggestions.slice(0, 4) as s (s.label)}
                  {@const SugIcon = suggestionIcon(s.label)}
                  <button
                    type="button"
                    class="group flex min-h-8 min-w-0 items-center gap-2.5 rounded-md px-2.5 text-left text-ui-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
                    disabled={loading}
                    title={s.prompt}
                    onclick={() => void send([s.prompt])}
                  >
                    <SugIcon class="size-4 shrink-0" />
                    <span class="min-w-0 truncate">{s.label}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <!-- gap-2, not gap-6: every message already carries a laid-out (invisible
               until hover) actions footer, so a large gap on top of it was paying
               for the same separation twice and left paragraphs stranded ~75px
               apart. The footer IS the breathing room. -->
          <div bind:this={msgListEl} class={cn("relative flex flex-col gap-2 py-5", !showTabBar && "pt-12")} data-studio-selectable="text">
            {#each items as item, idx (item.id)}
              {#if !drawsNothing(item)}
              <!-- content-visibility:auto lets the browser skip layout/paint for
                   off-screen messages (markdown, code, mermaid, charts), so scrolling
                   a long conversation stays smooth. `auto` intrinsic-size remembers each
                   message's real height once rendered, avoiding re-scroll jumps.
                   Skipped for the transient thinking/executing/streaming rows: they're
                   always at the bottom (visible) and their ping/bounce animations would
                   be clipped by the paint containment cv:auto adds. -->
              <div data-item-id={item.id} class={cn(showTabBar ? "scroll-mt-3" : "scroll-mt-12", idx >= enterFrom && "ai-enter", "")}>
              <!-- ── User message ───────────────────────── -->
              {#if item.kind === "user"}
                <div class="group/msg flex flex-col items-end px-3.5 pt-4">
                  <div
                    class="ai-user-bubble max-w-[80%] rounded-2xl border border-border/50 bg-card px-4 py-2.5 whitespace-pre-wrap break-words text-foreground"
                  >
                    {item.text}
                  </div>
                  {@render msgActions(item.id, item.text, item.ts, "end", false)}
                </div>

                <!-- ── Thinking ───────────────────────────── -->
              {:else if item.kind === "thinking"}
                {@render agentIndicator()}

                <!-- ── A chart or diagram being generated ── -->
              {:else if item.kind === "generating"}
                <div class="min-w-0 px-3.5">{@render generatingCard(item.what)}</div>

                <!-- ── Streaming ──────────────────────────── -->
              {:else if item.kind === "streaming"}
                <!-- No avatar column: the reply is the only thing on this side of
                     the transcript, so a per-message badge added a gutter that
                     bought nothing. The user's own turns are the contrast. -->
                {@const split = splitStreamingDiagram(displayStreamingContent)}
                <div class="min-w-0 px-3.5">
                  {#if streamingParts.plan}{@render planChecklist(streamingParts.plan, [])}{/if}
                  {#if split}
                    {#if split.before.trim()}<AiMarkdown content={split.before.trim()} streaming onrender={scrollBottomSoon} />{/if}
                    <div class="my-2">{@render generatingCard(split.kind === "chart" ? "chart" : "diagram")}</div>
                    {#if split.after.trim()}<AiMarkdown content={split.after.trim()} streaming onrender={scrollBottomSoon} />{/if}
                  {:else}
                  <AiMarkdown
                    content={displayStreamingContent}
                    streaming
                    onrender={scrollBottomSoon}
                  />
                  {/if}
                </div>

                <!-- ── Executing (tool call in progress) ──── -->
              {:else if item.kind === "executing"}
                {@const meta = execMeta(item.op, item.sql)}
                <!-- One line, one animation, and the same px-3.5 gutter as every
                     other row - it used to sit flush left of the text column with a
                     ping halo AND three bouncing dots competing for attention. -->
                <div class="flex items-center gap-2 px-3.5">
                  <span
                    class="relative flex size-5 shrink-0 items-center justify-center rounded-full bg-primary/10 ring-1 ring-inset ring-primary/20"
                  >
                    <span
                      class="absolute inset-0 animate-ping rounded-full bg-primary/10 [animation-duration:1.8s]"
                    ></span>
                    {#if item.op === "schema" || item.op === "describe"}
                      <Layers class="relative size-3 shrink-0 text-primary" />
                    {:else if item.op === "diagram"}
                      <GitBranch class="relative size-3 shrink-0 text-primary" />
                    {:else}
                      <Database class="relative size-3 shrink-0 text-primary" />
                    {/if}
                  </span>
                  <span class="shrink-0 text-ui-xs font-medium text-foreground/70">{meta.label}</span>
                  <span class="min-w-0 flex-1 truncate font-mono text-ui-3xs text-muted-foreground">
                    {#if item.op === "query" || item.op === "run"}{item.sql.trim().slice(0, 120)}{:else}{meta.detail}{/if}
                  </span>
                </div>

                <!-- ── A turn's steps: plan, timeline, summary ─── -->
              {:else if item.kind === "work"}
                {@const running = item.endedAt === null && loading}
                {@const groups = groupRetries(item.steps)}
                {@const failed = groups.filter((g) => g.status === "error").length}
                {@const isOpen = isWorkOpen(item, running)}
                {@const outcome = running ? { text: "", sql: "" } : summarizeSteps(item.steps)}
                {@const summary = outcome.text || workSummary(item.steps)}
                <div class="min-w-0 px-3.5">
                  {#if item.plan?.length}
                    {@render planChecklist(item.plan, planProgress(item.plan, item.steps, !running))}
                  {/if}
                  <!-- One quiet row: state icon, time, what was done, chevron.
                       Once the turn ends it says what changed, and the SQL that
                       did it is one click away. -->
                  <div class="flex min-w-0 items-center gap-1">
                    <button
                      type="button"
                      class="group/work -ml-1.5 inline-flex h-7 min-w-0 items-center gap-2 rounded-md px-1.5 text-ui-xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                      aria-expanded={isOpen}
                      onclick={() => toggleWorkItem(item, running)}
                    >
                      {#if running}
                        <!-- A CSS ring on its own compositor layer: it keeps turning
                             while the main thread is busy streaming the reply. -->
                        <span class="size-3.5 shrink-0 animate-spin rounded-full border-[1.75px] border-current/25 border-t-current will-change-transform" aria-hidden="true"></span>
                      {:else if failed}
                        <CircleAlert class="size-3.5 shrink-0 text-destructive" />
                      {:else}
                        <CircleCheck class="size-3.5 shrink-0 text-muted-foreground group-hover/work:text-foreground" />
                      {/if}
                      <span class={cn("shrink-0 tabular-nums", running && "text-foreground")}>{running ? "Working" : "Worked"} for {formatElapsed((item.endedAt ?? (running ? nowMs : item.startedAt)) - item.startedAt)}</span>
                      {#if summary}
                        <span class="truncate">· {summary}</span>
                      {/if}
                      {#if failed}
                        <span class="shrink-0 text-destructive">· {failed} failed</span>
                      {/if}
                      <ChevronRight class={cn("size-3.5 shrink-0", isOpen && "rotate-90")} />
                    </button>
                    {#if outcome.sql}
                      <button
                        type="button"
                        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                        title="Copy SQL"
                        aria-label="Copy the SQL this turn ran"
                        onclick={() => void copyWithFeedback(`${item.id}:sql`, outcome.sql)}
                      >
                        {#if copiedKey === `${item.id}:sql`}<Check class="size-3.5 shrink-0" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
                      </button>
                    {/if}
                  </div>
                  <!-- Opens and shuts at once: it is toggled often and follows
                       the turn by itself, so a height animation would only lag. -->
                  {#if isOpen}
                    <ol class="mt-1 mb-2 ml-[0.4375rem] flex flex-col gap-1 border-l border-border/50 pl-3.5">
                      {#each groups as group (group.step.id)}
                        {#if group.step.op === "note"}
                          <li class="py-0.5 text-ui-xs leading-relaxed whitespace-pre-wrap break-words text-muted-foreground">{group.step.detail}</li>
                        {:else}
                          <li class="flex min-w-0 flex-col gap-1">
                            {@render stepRow(group.step)}
                            {#if group.retries.length}
                              <div class="ml-[0.4375rem] flex flex-col gap-1 border-l border-border/50 pl-3">
                                <span class={cn("text-ui-2xs", group.status === "error" ? "text-destructive" : "text-muted-foreground")}>{retryLabel(group)}</span>
                                {#each group.retries as retry (retry.id)}
                                  {@render stepRow(retry)}
                                {/each}
                              </div>
                            {/if}
                          </li>
                        {/if}
                      {/each}
                    </ol>
                  {/if}
                </div>

                <!-- ── Assistant message ──────────────────── -->
              {:else if item.kind === "assistant"}
                <div class="group/msg min-w-0 px-3.5">
                  <div class="flex min-w-0 flex-col gap-2">
                    {#each item.parts as part, pi}
                      {#if part.type === "text"}
                        <AiMarkdown content={part.content} />
                      {:else if part.type === "chart"}
                        <!-- A chart written as a ```chart block (local agents),
                             drawn where it was written. -->
                        <div class="group/chart overflow-hidden rounded-lg border border-border/60">
                          <div class="flex h-8 items-center gap-2 border-b border-border/40 bg-muted/20 pl-3 pr-1.5">
                            <BarChart3 class="size-3.5 shrink-0 text-muted-foreground" />
                            <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground/80">{part.spec.title || "Chart"}</span>
                            <span class="shrink-0 text-ui-3xs capitalize text-muted-foreground">{String(part.spec.type).replace(/-/g, " ")}</span>
                            <button
                              type="button"
                              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                              title="Fullscreen"
                              aria-label="Fullscreen"
                              onclick={() => openChartFullscreen(part.spec)}
                            ><Maximize2 class="size-3.5 shrink-0" /></button>
                          </div>
                          <div class="px-2 py-2">
                            <AiChartRenderer spec={part.spec} noTitle={true} />
                          </div>
                        </div>
                      {:else if part.type === "pending"}
                        {#if part.kind === "diagram"}{@render generatingCard("diagram")}{:else if part.kind === "chart"}{@render generatingCard("chart")}{/if}
                      {:else if part.type === "mermaid"}
                        {@const dKind = diagramKind(part.content)}
                        <div
                          class="mermaid-output overflow-hidden rounded-lg border border-border/60"
                        >
                          <!-- One quiet row: what the diagram is, then icon actions.
                               Five text buttons and a usage hint competed with
                               the diagram they were there to serve. -->
                          <div
                            class="flex h-8 items-center gap-1 border-b border-border/40 bg-muted/20 pl-3 pr-1.5"
                            title="Drag to pan · Ctrl/⌘ + scroll to zoom · double-click to reset"
                          >
                            <GitBranch class="size-3.5 shrink-0 text-muted-foreground" />
                            <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground/80">{dKind}</span>
                            {#each [
                              { label: "Copy source", icon: Copy, run: () => copyText(part.content) },
                              { label: "Export SVG", icon: ArrowDownToLine, run: () => void exportDiagramSvg() },
                              { label: "Export PNG", icon: ImageIcon, run: () => void exportDiagramPng() },
                              { label: "Save to Diagrams", icon: Save, run: () => {
                                  const name = part.content.trim().split("\n")[0].replace(/[^a-zA-Z0-9 _-]/g, "").trim() || "Diagram";
                                  saveDiagram(name, part.content);
                                  toast.success("Diagram saved", {
                                    description: "View it in the Diagrams page",
                                    action: onopendiagramspage ? { label: "Open", onClick: onopendiagramspage } : undefined,
                                  });
                                } },
                              { label: "Fullscreen", icon: Maximize2, run: () => openDiagramFullscreen(part.content) },
                            ] as a (a.label)}
                              {@const Glyph = a.icon}
                              <button
                                type="button"
                                class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                                title={a.label}
                                aria-label={a.label}
                                onclick={a.run}
                              ><Glyph class="size-3.5 shrink-0" /></button>
                            {/each}
                          </div>
                          <div class="mermaid-canvas" use:mermaidInteractive>
                            {@html processMermaidSvg(part.content)}
                          </div>
                        </div>
                      {:else if part.type === "sql"}
                        {@const sqlKey = `${item.id}-${pi}`}
                        {@const sqlOpen = !collapsed.has(sqlKey)}
                        {@const sqlWrites = isWriteSql(part.content)}
                        <div
                          class="overflow-hidden rounded-lg border border-border/50 bg-card/30"
                        >
                          <!-- Header. When the block is open the code below IS the
                               query, so the header drops the preview instead of
                               printing the same statement twice; collapsed, the
                               preview is the only thing standing in for it. -->
                          <!-- No border-b here: AiSqlBlock draws its own border-t when
                               open, and the two stacked into a 2px double rule. -->
                          <div
                            class="group/sqlbar flex items-center gap-2 bg-muted/8 px-2.5 py-1.5"
                          >
                            <button
                              type="button"
                              class="flex min-w-0 flex-1 items-center gap-2 text-left"
                              onclick={() => toggleCollapse(sqlKey)}
                              title={sqlOpen ? "Collapse" : "Expand"}
                            >
                              <span
                                class="flex size-4 shrink-0 items-center justify-center text-muted-foreground transition-colors group-hover/sqlbar:text-muted-foreground"
                              >
                                {#if sqlOpen}<ChevronDown class="size-3 shrink-0" />{:else}<ChevronRight class="size-3 shrink-0" />{/if}
                              </span>
                              <!-- The leading keyword, tinted when the statement
                                   writes: the one thing worth knowing before Run. -->
                              <span
                                class={cn(
                                  "shrink-0 rounded font-mono text-ui-3xs font-semibold uppercase tracking-[0.06em] px-1 py-px",
                                  sqlWrites
                                    ? "bg-warning/12 text-warning"
                                    : "bg-muted/50 text-muted-foreground",
                                )}>{sqlStatementKind(part.content)}</span
                              >
                              {#if !sqlOpen}
                                <span
                                  class="min-w-0 truncate font-mono text-ui-xs text-muted-foreground"
                                  >{part.content.trim().replace(/\s+/g, " ").slice(0, 90)}</span
                                >
                              {/if}
                            </button>
                            <!-- Copy/edit stay quiet until hover; Run is the point of
                                 the block, so it is always legible. -->
                            <div class="flex shrink-0 items-center gap-0.5">
                              <button
                                type="button"
                                class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity duration-150 hover:bg-accent hover:text-foreground group-hover/sqlbar:opacity-100 focus-visible:opacity-100"
                                title="Copy SQL"
                                onclick={() => copyText(part.content)}
                                ><Copy class="size-3 shrink-0" /></button
                              >
                              <button
                                type="button"
                                class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity duration-150 hover:bg-accent hover:text-foreground group-hover/sqlbar:opacity-100 focus-visible:opacity-100"
                                title="Write to editor"
                                onclick={() => onwritesql(part.content)}
                                ><PenLine class="size-3 shrink-0" /></button
                              >
                              <button
                                type="button"
                                class= "field-surface ml-0.5 inline-flex h-6 items-center gap-1 bg-muted/40 px-2 text-ui-2xs font-medium text-foreground/80 transition-colors hover:border-primary/40 hover:bg-primary hover:text-primary-foreground disabled:opacity-40"
                                disabled={loading}
                                onclick={() => void runSqlBlock(part.content)}
                                ><Play class="size-3 shrink-0 fill-current" />Run</button
                              >
                            </div>
                          </div>
                          <AiSqlBlock sql={part.content} open={sqlOpen} />
                        </div>
                      {:else if part.type === "error"}
                        <div
                          class="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/6 px-3 py-2.5 text-ui-xs text-destructive"
                        >
                          <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
                          <span>{part.content}</span>
                        </div>
                      {:else if part.type === "confirm_prompt"}
                        <div
                          class="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning/6 px-3 py-2.5 text-ui-xs text-warning"
                        >
                          <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
                          <span>{part.content}</span>
                        </div>
                      {:else if part.content?.trim()}
                        {@const codeKey = `${item.id}-${pi}`}
                        {@const codeOpen = !collapsed.has(codeKey)}
                        <div
                          class="overflow-hidden rounded-lg border border-border/60"
                        >
                          <div
                            class="flex items-center justify-between gap-2 border-b border-border/40 bg-muted/20 px-3 py-1.5"
                          >
                            <button
                              type="button"
                              class="flex items-center gap-1.5 text-ui-xs text-muted-foreground hover:text-foreground"
                              onclick={() => toggleCollapse(codeKey)}
                            >
                              {#if codeOpen}<ChevronDown
                                  class="size-3 shrink-0"
                                />{:else}<ChevronRight class="size-3 shrink-0" />{/if}
                              <span class="font-mono"
                                >{part.lang || "code"}</span
                              >
                            </button>
                            <button
                              type="button"
                              class="inline-flex h-6 items-center gap-1 rounded-md px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
                              onclick={() => copyText(part.content)}
                              ><Copy class="size-3 shrink-0" />Copy</button
                            >
                          </div>
                          {#if codeOpen}
                            <div class="bg-muted/10">
                              <ShikiBlock
                                code={part.content}
                                lang={part.lang || "plaintext"}
                                embedded
                              />
                            </div>
                          {/if}
                        </div>
                      {/if}
                    {/each}
                  </div>
                  {@render msgActions(
                    item.id,
                    assistantMarkdown(item.parts),
                    item.ts,
                    "start",
                    true,
                  )}
                </div>

                <!-- ── Query result card ──────────────────── -->
              {:else if item.kind === "result"}
                <!-- Hidden by the Agent setting, except when the query failed:
                     suppressing an error would leave the model's recovery in the
                     transcript with nothing explaining what it recovered from. -->
                {#if $appAgentQueryCards || item.error}
                {@const resOpen = openResultId === item.id}
                <!-- One neutral card for every outcome. Tinting the whole
                     container per state (red for errors, primary for schema)
                     gave a transcript of coloured blocks competing with the
                     prose; state now reads from the leading icon alone. -->
                <div
                  class="mx-3.5 overflow-hidden rounded-lg border border-border/40 text-ui-xs"
                >
                  <div
                    class="group/res flex w-full items-center gap-1.5 px-3 py-1.5 transition-colors hover:bg-muted/20 {resOpen
                      ? 'border-b border-border/30'
                      : ''}"
                  >
                    <button
                      type="button"
                      class="flex min-w-0 flex-1 items-center gap-2 text-left"
                      onclick={() => toggleResult(item.id)}
                    >
                      {#if resOpen}<ChevronDown
                          class="size-3 shrink-0 text-muted-foreground"
                        />{:else}<ChevronRight
                          class="size-3 shrink-0 text-muted-foreground"
                        />{/if}
                      {#if item.error}
                        <AlertTriangle class="size-3 shrink-0 text-destructive" />
                      {:else}
                        <Table2
                          class="size-3 shrink-0 {item.isSchema
                            ? 'text-primary'
                            : 'text-muted-foreground'}"
                        />
                      {/if}
                      <span
                        class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground"
                        >{item.sql || "Query"}</span
                      >
                    </button>
                    {#if item.sql}
                      <button
                        type="button"
                        class="hit-area inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/res:opacity-100 hover:bg-accent hover:text-foreground"
                        title="Copy SQL"
                        onclick={() => copyText(item.sql)}
                        ><Copy class="size-3 shrink-0" /></button
                      >
                      <button
                        type="button"
                        class="hit-area inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/res:opacity-100 hover:bg-accent hover:text-foreground"
                        title="Write to editor"
                        onclick={() => onwritesql(item.sql)}
                        ><PenLine class="size-3 shrink-0" /></button
                      >
                    {/if}
                    {#if !item.error && item.rows.length > 0}
                      <!-- The grid as a markdown table - pasteable straight back
                           into a chat, unlike the rendered card. -->
                      <button
                        type="button"
                        class="hit-area inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/res:opacity-100 hover:bg-accent hover:text-foreground"
                        title="Copy table as Markdown"
                        onclick={() =>
                          void copyWithFeedback(
                            `${item.id}:table`,
                            rowsToMarkdown(item.columns, item.rows),
                          )}
                      >
                        {#if copiedKey === `${item.id}:table`}
                          <Check class="size-3 shrink-0 text-success" strokeWidth={2.5} />
                        {:else}
                          <Table2 class="size-3 shrink-0" />
                        {/if}
                      </button>
                    {/if}
                    {#if !item.error}
                      <span
                        class="shrink-0 rounded bg-muted/40 px-1.5 py-0.5 text-ui-3xs tabular-nums text-muted-foreground"
                        >{formatCompactCount(item.total)}
                        {item.total === 1 ? "row" : "rows"}</span
                      >
                    {/if}
                  </div>
                  {#if resOpen}
                    {#if item.error}
                      <!-- The cause on one line, in the body text colour. A
                           red-on-red block shouted the loudest thing in the
                           transcript at what is usually a recoverable typo the
                           model fixes on its next turn; the rail carries the
                           severity instead. Raw driver text stays one click away. -->
                      <div class="px-3 py-2.5">
                        <p class="border-l-2 border-destructive/50 pl-2.5 font-mono text-ui-2xs leading-relaxed text-foreground/70">
                          {humanizeDbError(item.error)}
                        </p>
                        {#if humanizeDbError(item.error) !== item.error.replace(/^Error:\s*/i, "").trim()}
                          <button
                            type="button"
                            class="mt-1.5 pl-2.5 text-ui-3xs text-muted-foreground transition-colors hover:text-foreground"
                            onclick={() => (rawErrorId = rawErrorId === item.id ? null : item.id)}
                          >{rawErrorId === item.id ? "Hide" : "Show"} raw error</button>
                          {#if rawErrorId === item.id}
                            <p class="mt-1.5 pl-2.5 font-mono text-ui-3xs leading-relaxed break-all text-muted-foreground">
                              {item.error}
                            </p>
                          {/if}
                        {/if}
                      </div>
                    {:else if item.rows.length === 0}
                      <p
                        class="px-3 py-3 text-center text-ui-xs italic text-muted-foreground"
                      >
                        No rows returned.
                      </p>
                    {:else}
                      <div class="overflow-x-auto">
                        <DataTable
                          columns={item.columns}
                          rows={item.rows.slice(0, 15)}
                          embedded
                          showSelection={false}
                        />
                      </div>
                      {#if item.total > 15}
                        <p
                          class="border-t border-border/20 px-3 py-1.5 text-ui-3xs text-muted-foreground"
                        >
                          Showing 15 of {formatCompactCount(item.total)} rows{item.capped
                            ? ` (limited to ${AI_ROW_LIMIT})`
                            : ""}
                        </p>
                      {/if}
                      <!-- Export the result as a downloadable file -->
                      <div
                        class="flex items-center gap-1 border-t border-border/20 px-3 py-1.5"
                      >
                        <Download
                          class="mr-1 size-3 shrink-0 text-muted-foreground"
                        />
                        {#each [["csv", "CSV"], ["json", "JSON"], ["md", "Markdown"]] as [fmt, label]}
                          <button
                            type="button"
                            class="rounded px-1.5 py-0.5 text-ui-3xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                            onclick={() => exportResultAs(item, fmt)}
                            title={`Download as ${label}`}>{label}</button
                          >
                        {/each}
                      </div>
                    {/if}
                  {/if}
                </div>
                {/if}

                <!-- ── Web search / page fetch ─────────────── -->
              {:else if item.kind === "web"}
                <div class="mx-3.5 overflow-hidden rounded-lg border border-border/40 text-ui-xs">
                  <div class="flex items-center gap-1.5 px-3 py-1.5">
                    {#if item.error}
                      <AlertTriangle class="size-3 shrink-0 text-destructive" />
                    {:else if item.mode === "search"}
                      <Search class="size-3 shrink-0 text-muted-foreground" />
                    {:else}
                      <Globe class="size-3 shrink-0 text-muted-foreground" />
                    {/if}
                    <span class="min-w-0 flex-1 truncate text-ui-2xs text-muted-foreground">
                      {item.mode === "search" ? "Searched" : "Read"} · {item.label}
                    </span>
                    {#if item.loading}
                      <span class="shrink-0 text-ui-3xs text-muted-foreground">…</span>
                    {:else if item.mode === "search" && !item.error}
                      <span class="shrink-0 rounded bg-muted/40 px-1.5 py-0.5 text-ui-3xs tabular-nums text-muted-foreground">
                        {item.hits.length}
                        {item.hits.length === 1 ? "result" : "results"}
                      </span>
                    {/if}
                  </div>
                  {#if item.error}
                    <p class="border-t border-border/30 px-3 py-2">
                      <span class="border-l-2 border-destructive/50 pl-2.5 font-mono text-ui-2xs text-foreground/70">{item.error}</span>
                    </p>
                  {:else if item.mode === "search" && item.hits.length}
                    <ul class="border-t border-border/30">
                      {#each item.hits as hit (hit.url)}
                        <li class="border-b border-border/15 px-3 py-1.5 last:border-b-0">
                          <button
                            type="button"
                            class="block w-full min-w-0 text-left"
                            onclick={() => void openExternal(hit.url)}
                            title={hit.url}
                          >
                            <span class="block truncate text-ui-2xs text-foreground/80 hover:underline">{hit.title}</span>
                            {#if hit.snippet}
                              <span class="mt-0.5 line-clamp-2 block text-ui-3xs leading-relaxed text-muted-foreground">{hit.snippet}</span>
                            {/if}
                          </button>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </div>

                <!-- ── Confirm dialog ──────────────────────── -->
              {:else if item.kind === "confirm"}
                <div
                  class="mx-3.5 overflow-hidden rounded-lg border border-destructive/30 bg-destructive/4"
                >
                  <div
                    class="flex items-center gap-2 border-b border-destructive/20 bg-destructive/8 px-3 py-2"
                  >
                    <AlertTriangle class="size-3.5 shrink-0 text-destructive" />
                    <span class="text-ui-xs font-medium text-destructive"
                      >Confirm destructive operation</span
                    >
                  </div>
                  <pre
                    class="px-3 py-2.5 font-mono text-ui-xs text-foreground whitespace-pre-wrap">{item.sql}</pre>
                  <div
                    class="flex items-center justify-between gap-2 border-t border-destructive/15 px-3 py-2"
                  >
                    <p class="text-ui-xs text-muted-foreground">
                      This cannot be undone.
                    </p>
                    <div class="flex gap-1.5">
                      <button
                        type="button"
                        class= "field-surface inline-flex h-7 items-center px-3 text-ui-xs text-muted-foreground hover:bg-accent"
                        onclick={() => item.resolve(false)}>Cancel</button
                      >
                      <button
                        type="button"
                        class="inline-flex h-7 items-center rounded-md bg-destructive px-3 text-ui-xs font-medium text-destructive-foreground hover:opacity-90"
                        onclick={() => item.resolve(true)}>Execute</button
                      >
                    </div>
                  </div>
                </div>

                <!-- ── Chart card ─────────────────────────── -->
              {:else if item.kind === "chart"}
                <div class="group/chart mx-3.5">
                  {#if item.error}
                    <p
                      class="flex items-center gap-1.5 text-ui-xs text-destructive"
                    >
                      <AlertTriangle class="size-3 shrink-0" />{item.error}
                    </p>
                  {:else}
                    <!-- Floating header: no background, no border, appears on hover -->
                    <div class="mb-0.5 flex items-center gap-1.5">
                      <span
                        class="min-w-0 flex-1 truncate font-mono text-ui-2xs font-medium text-foreground/60"
                        >{item.spec.title || ""}</span
                      >
                      <span
                        class="font-mono text-ui-3xs capitalize text-muted-foreground"
                        >{item.spec.type}</span
                      >
                      <div
                        class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/chart:opacity-100"
                      >
                        <!-- Reset / re-centre -->
                        <button
                          type="button"
                          class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                          title="Reset view (or double-click chart)"
                          onclick={() => void resetChartView(item.id)}
                          ><RotateCcw class="size-3 shrink-0" /></button
                        >
                        <!-- Fullscreen / zoom mode -->
                        <button
                          type="button"
                          class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                          title="Zoom / fullscreen"
                          onclick={() => openChartFullscreen(item.spec)}
                          ><Maximize2 class="size-3 shrink-0" /></button
                        >
                        <button
                          type="button"
                          class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                          title="Download PNG"
                          onclick={() =>
                            void saveChartPng(
                              `[data-chart-id="${item.id}"] canvas`,
                              item.spec.title,
                            )}><ArrowDownToLine class="size-3 shrink-0" /></button
                        >
                        <button
                          type="button"
                          class="inline-flex size-6 items-center justify-center rounded-md transition-colors {savedChartIds.has(
                            item.id,
                          )
                            ? 'text-primary'
                            : 'text-muted-foreground hover:text-foreground'}"
                          title={savedChartIds.has(item.id)
                            ? "Saved to Charts"
                            : "Save to Charts"}
                          onclick={() => {
                            if (savedChartIds.has(item.id)) return;
                            const chartIdx = items.findIndex(
                              (i) => i.id === item.id,
                            );
                            const prevResult = items
                              .slice(0, chartIdx)
                              .reverse()
                              .find(
                                (i) =>
                                  i.kind === "result" &&
                                  !(/** @type {any} */ (i).error),
                              );

                            // Build the full ECharts option so previews render immediately
                            const spec = item.spec;
                            // Saved chats can hold a spec from before data was normalised.
                            const specRows = chartRows(spec.data, spec);
                            const keys = specRows.length
                              ? Object.keys(specRows[0] ?? {})
                              : [];
                            const cols = keys.map((k) => {
                              const sample = specRows.find(
                                (r) => r[k] != null,
                              )?.[k];
                              const dt =
                                typeof sample === "number"
                                  ? "numeric"
                                  : typeof sample === "string" &&
                                      /^\d{4}-\d{2}/.test(sample)
                                    ? "timestamp"
                                    : "text";
                              return { name: k, dataType: dt, data_type: dt };
                            });
                            const rows = specRows.map((obj) =>
                              keys.map((k) => obj[k]),
                            );
                            let previewOption = {};
                            try {
                              previewOption = buildOption({
                                type: spec.type ?? "bar",
                                columns: cols,
                                rows,
                                xCol: spec.x_col ?? cols[0]?.name ?? "",
                                yCol: spec.y_col ?? cols[1]?.name ?? "",
                                zCol: spec.z_col || undefined,
                                groupCol: spec.group_col || undefined,
                                isDark: $isCurrentThemeDark,
                                noTitle: true,
                              });
                            } catch {}

                            saveChart({
                              name: spec.title || "AI Chart",
                              group: "Chat Saved",
                              connectionId,
                              sql:
                                prevResult?.kind === "result"
                                  ? /** @type {any} */ (prevResult).sql
                                  : "",
                              config: {
                                type: spec.type,
                                xCol: spec.x_col ?? "",
                                yCol: spec.y_col ?? "",
                                zCol: spec.z_col,
                                groupCol: spec.group_col,
                                title: spec.title,
                              },
                              previewOption,
                              // Keep the full AI spec so AiChartRenderer can
                              // re-render types (meter, choropleth) that don't
                              // produce an ECharts previewOption.
                              aiSpec: spec,
                            });
                            savedChartIds = new Set([
                              ...savedChartIds,
                              item.id,
                            ]);
                            toast.success("Chart saved", {
                              description: spec.title || "Chart",
                            });
                          }}
                        >
                          {#if savedChartIds.has(item.id)}
                            <Check class="size-3 shrink-0" />
                          {:else}
                            <Save class="size-3 shrink-0" />
                          {/if}
                        </button>
                      </div>
                    </div>
                    <!-- Chart body, fills full width, height by type -->
                    <div
                      data-chart-id={item.id}
                      style="height:{[
                        'choropleth',
                        'dendrogram',
                        'tree',
                        'sankey',
                      ].includes(item.spec.type)
                        ? 480
                        : 360}px; width:100%"
                    >
                      <AiChartRenderer spec={item.spec} noTitle={true} />
                    </div>
                  {/if}
                </div>

                <!-- ── Diagram (render_diagram tool result) ── -->
              {:else if item.kind === "diagram"}
                <div
                  class="group/diag mx-3.5 mermaid-output overflow-hidden rounded-lg border border-border/60"
                >
                  <div
                    class="flex items-center justify-between gap-2 border-b border-border/40 bg-muted/20 px-3 py-1.5"
                  >
                    <div class="flex items-center gap-2 min-w-0">
                      <GitBranch
                        class="size-3 shrink-0 text-muted-foreground"
                      />
                      <span
                        class="truncate text-ui-3xs font-medium text-foreground/70"
                        >{item.title}</span
                      >
                    </div>
                    <div
                      class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/diag:opacity-100"
                    >
                      <span
                        class="hidden text-ui-3xs text-muted-foreground sm:block mr-1"
                        >drag · Ctrl+scroll zoom</span
                      >
                      <button
                        type="button"
                        class="inline-flex h-5 items-center gap-1 rounded-md px-1.5 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"
                        onclick={() => copyText(item.code)}
                        title="Copy source"
                        ><Copy class="size-3 shrink-0" />Source</button
                      >
                      <button
                        type="button"
                        class="inline-flex h-5 items-center gap-1 rounded-md px-1.5 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"
                        onclick={() => void exportDiagramSvg()}
                        title="Export SVG"
                        ><ArrowDownToLine class="size-3 shrink-0" />SVG</button
                      >
                      <button
                        type="button"
                        class="inline-flex h-5 items-center gap-1 rounded-md px-1.5 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"
                        onclick={() => void exportDiagramPng()}
                        title="Export PNG"
                        ><ArrowDownToLine class="size-3 shrink-0" />PNG</button
                      >
                      <button
                        type="button"
                        class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
                        onclick={() => openDiagramFullscreen(item.code)}
                        title="Fullscreen"><Maximize2 class="size-3 shrink-0" /></button
                      >
                      <button
                        type="button"
                        class={savedDiagramIds.has(item.id)
                          ? "hit-area inline-flex size-5 items-center justify-center rounded-md text-success"
                          : "inline-flex h-5 items-center gap-1 rounded-md px-1.5 text-ui-3xs text-muted-foreground hover:bg-accent hover:text-foreground"}
                        title={savedDiagramIds.has(item.id) ? "Saved to Diagrams library" : "Save to Diagrams library"}
                        disabled={savedDiagramIds.has(item.id)}
                        onclick={() => {
                          if (savedDiagramIds.has(item.id)) return;
                          saveDiagram(item.title || "Diagram", item.code);
                          savedDiagramIds = new Set([...savedDiagramIds, item.id]);
                          toast.success("Diagram saved", {
                            description: "View it in the Diagrams page",
                            action: onopendiagramspage
                              ? { label: "Open", onClick: onopendiagramspage }
                              : undefined,
                          });
                        }}
                        >{#if savedDiagramIds.has(item.id)}<Check class="size-3 shrink-0" strokeWidth={2.5} />{:else}<Save class="size-3 shrink-0" />Save{/if}</button
                      >
                    </div>
                  </div>
                  <div class="mermaid-canvas" use:mermaidInteractive>
                    {@html processMermaidSvg(item.code)}
                  </div>
                </div>
              {/if}
              </div>
              {/if}
            {/each}

            {#if showWorking}
              {@render agentIndicator()}
            {/if}

            <!-- Watched by the observer above to decide whether the transcript is
                 pinned to the bottom. A zero-height element is enough, and it
                 costs nothing per frame. -->
            <div bind:this={bottomSentinel} aria-hidden="true" class="h-px w-full shrink-0"></div>
            {#if tailSpace > 0}
              <div aria-hidden="true" class="w-full shrink-0" style="height:{tailSpace}px"></div>
            {/if}
          </div>
        {/if}
      </div>
      </div>
    </div>

    <!-- Error bar.
         Red is the signal, not the whole surface: the icon and the hairline
         carry it, and the words stay in the normal text colours so the message
         is read rather than braced against. The provider's own sentence is the
         message now (see describeAiError), so most failures have nothing left
         to hide behind a Details toggle. -->
    {#if error}
      {@const failure = describeAiError(error)}
      <!-- A card in the composer's column, not a bar across the window: it
           belongs to the turn that failed, and sits where the eye already is. -->
      <div class={cn("shrink-0", mode === "full" ? "px-6 pt-2" : "px-3 pt-2")}>
      <div role="alert" class={cn("rounded-lg border border-destructive/25 bg-destructive/[0.05] px-3.5 py-3", mode === "full" && "mx-auto w-full max-w-3xl")}>
        <div class="flex items-start gap-2.5">
          <AlertTriangle class="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden="true" />
          <div class="min-w-0 flex-1">
            <p class="text-ui-sm font-medium text-foreground">
              {failure.title}
            </p>
            {#if failure.hint}
              <p class="mt-0.5 text-pretty text-ui-xs leading-relaxed text-muted-foreground">
                {failure.hint}
              </p>
            {/if}
            {#if failure.status === 401 || failure.status === 403}
              <button
                type="button"
                onclick={onopenmodelsettings}
                class="mt-2 inline-flex h-7 items-center rounded-md border border-border bg-background px-2.5 text-ui-xs font-medium text-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
              >Open AI settings</button>
            {/if}
            {#if failure.detail}
              <button
                type="button"
                onclick={() => (errorDetailOpen = !errorDetailOpen)}
                class="mt-1 inline-flex items-center gap-1 rounded text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
              >
                <ChevronRight class={cn('size-3 transition-transform duration-150', errorDetailOpen && 'rotate-90')} />
                {errorDetailOpen ? 'Hide details' : 'Details'}
              </button>
              {#if errorDetailOpen}
                <div class="mt-1.5 flex items-start gap-2">
                  <pre class="max-h-28 min-w-0 flex-1 overflow-auto whitespace-pre-wrap break-all rounded-md border border-border/50 bg-muted/30 p-2 font-mono text-ui-3xs leading-relaxed text-muted-foreground">{failure.detail}</pre>
                  <button
                    type="button"
                    onclick={() => copyErrorDetail(failure.detail)}
                    title="Copy the provider's response"
                    class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                  >
                    <Copy class="size-3 shrink-0" />
                  </button>
                </div>
              {/if}
            {/if}
          </div>
          <button
            type="button"
            class="hit-area inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={() => (error = "")}
            title="Dismiss"
            aria-label="Dismiss"
          >
            <X class="size-3.5 shrink-0" />
          </button>
        </div>
      </div>
      </div>
    {/if}

    <!-- Jump-to-bottom button: shown whenever user has scrolled away from bottom -->
    {#if userScrolledUp}
      <!-- A round arrow centred over the composer, as T3 and ChatGPT do; the
           labelled pill read as one more message lying across the text. -->
      <div class="pointer-events-none absolute inset-x-0 bottom-28 z-10 flex justify-center">
        <button
          type="button"
          onclick={jumpToBottom}
          class="ai-enter pointer-events-auto flex size-8 items-center justify-center rounded-full border border-border/70 bg-popover text-muted-foreground elevate-2-rim transition-colors hover:bg-accent hover:text-foreground"
          title="Jump to bottom"
          aria-label="Jump to bottom"
        >
          <ArrowDown class="size-4 shrink-0" />
        </button>
      </div>
    {/if}

    <!-- Input -->
    <!-- No rule above the composer: it reads as one floating field over the
         transcript, and the card's own border already separates the two. -->
    {#if items.length > 0}
    <div
      class="shrink-0 {mode === 'full' ? 'px-6 pb-5 pt-2' : 'px-3 pb-3 pt-2'}"
    >
      <div class={mode === "full" ? "mx-auto w-full max-w-3xl" : ""}>
        {@render composerCard()}
      </div>
    </div>
    {/if}
  </div>

  <!-- ── Right settings panel ─────────────────────────────────────── -->
  {#if settingsOpen}
    <aside
      class="flex w-80 shrink-0 flex-col border-l border-border/50 bg-panel"
    >
      <!-- Header, py-2 matches main chat header height so border-b aligns -->
      <div
        class="studio-chrome flex shrink-0 items-center border-b border-border/50 px-2 py-2"
        data-studio-chrome
      >
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onclick={() => (settingsOpen = false)}
          title="Close ({modKey}⇧,)"
        >
          <X class="size-3.5 shrink-0" />
        </button>
        <span class="flex-1 text-center text-ui-xs font-semibold tracking-wide"
          >Settings</span
        >
        <div class="size-7 shrink-0"></div>
      </div>

      <!-- Tab bar -->
      <div
        class="studio-chrome flex h-9 shrink-0 items-stretch border-b border-border/50"
        data-studio-chrome
        role="tablist"
        aria-label="AI settings"
      >
        {#each SETTINGS_TABS as tab (tab.id)}
          {@const Icon = tab.icon}
          {@const active = settingsTab === tab.id}
          <button
            type="button"
            role="tab"
            aria-selected={active}
            aria-label={tab.label}
            title={tab.label}
            class={cn(
              "relative flex flex-1 items-center justify-center gap-1.5 text-ui-xs transition-colors",
              active
                ? "text-foreground font-medium"
                : "text-muted-foreground hover:text-foreground",
            )}
            onclick={() =>
              (settingsTab = /** @type {'model'|'chat'|'skills'|'context'} */ (
                tab.id
              ))}
          >
            <Icon class="size-3.5 shrink-0" />
            <span class="tabular-nums"
              >{tab.id === "skills" && skills.length
                ? ` ${skills.length}`
                : ""}</span
            >
            {#if active}
              <span class="absolute bottom-0 left-0 right-0 h-px bg-primary"
              ></span>
            {/if}
          </button>
        {/each}
      </div>

      <!-- Panel content -->
      <div class="app-scroll flex min-h-0 flex-1 flex-col overflow-y-auto">
        <!-- ── Model tab ── -->
        {#if settingsTab === "model"}
          <div class="flex flex-col gap-3 p-4">
            <!-- Not configured warning -->
            {#if !settings.apiKey && !settings.baseUrl.includes("localhost")}
              <div
                class="flex items-start gap-2 rounded-lg border border-warning/25 bg-warning/6 px-3 py-2.5"
              >
                <AlertTriangle
                  class="mt-0.5 size-3.5 shrink-0 text-warning"
                />
                <p
                  class="text-ui-xs leading-relaxed text-warning"
                >
                  No API key configured.
                </p>
              </div>
            {/if}

            <!-- Active profile card -->
            {#if true}
              {@const activeProfile =
                $aiProfiles.find((p) => p.id === $activeProfileId) ??
                $aiProfiles[0]}
              <div
                class="flex flex-col divide-y divide-border/30 rounded-lg border border-border/50 bg-background/60 overflow-hidden"
              >
                {#each [{ label: "Profile", value: activeProfile?.name ?? "—", mono: false }, { label: "Model", value: settings.model || "—", mono: true }, { label: "Endpoint", value: settings.baseUrl, mono: true, truncate: true }, { label: "API key", value: settings.apiKey ? "•••• set" : "not set", mono: true }] as row}
                  <div
                    class="flex items-center justify-between gap-3 px-3 py-2"
                  >
                    <span class="shrink-0 text-ui-3xs text-muted-foreground"
                      >{row.label}</span
                    >
                    <span
                      class="min-w-0 {row.truncate ? 'truncate' : ''} {row.mono
                        ? 'font-mono text-ui-3xs'
                        : 'text-ui-xs'} text-right text-foreground/80"
                      >{row.value}</span
                    >
                  </div>
                {/each}
              </div>
            {/if}

            <button
              type="button"
              class= "field-surface flex h-8 w-full items-center justify-center gap-1.5 bg-background/60 text-ui-xs text-foreground transition-colors hover:bg-accent"
              onclick={onopenmodelsettings}
            >
              <Settings2 class="size-3.5 shrink-0 text-muted-foreground" />
              Configure model…
            </button>

            <p class="text-center text-ui-3xs text-muted-foreground">
              Shared with AI sidebar · <kbd class="font-mono">{modKey}I</kbd>
            </p>
          </div>

          <!-- ── Chat tab ── -->
        {:else if settingsTab === "chat"}
          <div class="flex flex-col gap-4 p-4">
            <!-- Temperature -->
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between">
                <label
                  for="cp-temperature"
                  class="text-ui-xs font-medium text-foreground"
                  >Temperature</label
                >
                <span class="font-mono text-ui-2xs text-muted-foreground"
                  >{$aiChatParams.temperature.toFixed(2)}</span
                >
              </div>
              <input
                id="cp-temperature"
                type="range"
                min="0"
                max="2"
                step="0.05"
                value={$aiChatParams.temperature}
                oninput={(e) =>
                  updateChatParams({
                    temperature: parseFloat(e.currentTarget.value),
                  })}
                class="w-full accent-primary"
              />
              <p class="text-ui-3xs text-muted-foreground">
                0 = deterministic · 1 = balanced · 2 = creative
              </p>
            </div>

            <!-- Top-K -->
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between">
                <label
                  for="cp-topk"
                  class="text-ui-xs font-medium text-foreground">Top-K</label
                >
                <div class="flex items-center gap-1.5">
                  <span class="font-mono text-ui-2xs text-muted-foreground"
                    >{$aiChatParams.topK ?? "off"}</span
                  >
                  <button
                    type="button"
                    class="font-mono text-ui-3xs text-muted-foreground hover:text-foreground"
                    onclick={() =>
                      updateChatParams({
                        topK: $aiChatParams.topK === null ? 40 : null,
                      })}
                    >{$aiChatParams.topK === null
                      ? "enable"
                      : "disable"}</button
                  >
                </div>
              </div>
              {#if $aiChatParams.topK !== null}
                <input
                  id="cp-topk"
                  type="range"
                  min="1"
                  max="100"
                  step="1"
                  value={$aiChatParams.topK}
                  oninput={(e) =>
                    updateChatParams({ topK: parseInt(e.currentTarget.value) })}
                  class="w-full accent-primary"
                />
              {/if}
              <p class="text-ui-3xs text-muted-foreground">
                Limits token sampling pool. Not supported by all providers.
              </p>
            </div>

            <!-- Max tokens -->
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between">
                <label
                  for="cp-maxtokens"
                  class="text-ui-xs font-medium text-foreground"
                  >Max tokens</label
                >
                <span class="font-mono text-ui-2xs text-muted-foreground"
                  >{$aiChatParams.maxTokens.toLocaleString()}</span
                >
              </div>
              <input
                id="cp-maxtokens"
                type="range"
                min="512"
                max="32768"
                step="512"
                value={$aiChatParams.maxTokens}
                oninput={(e) =>
                  updateChatParams({
                    maxTokens: parseInt(e.currentTarget.value),
                  })}
                class="w-full accent-primary"
              />
            </div>

            <!-- Custom instructions -->
            <div class="flex flex-col gap-1.5">
              <label
                for="cp-instructions"
                class="text-ui-xs font-medium text-foreground"
                >Custom instructions</label
              >
              <textarea
                id="cp-instructions"
                rows="5"
                placeholder="Always respond in Spanish. Focus on performance. Prefer CTEs over subqueries…"
                value={$aiChatParams.customInstructions}
                oninput={(e) =>
                  updateChatParams({
                    customInstructions: e.currentTarget.value,
                  })}
                class="w-full resize-none rounded-lg border-2 border-border bg-background/60 px-2.5 py-2 font-mono text-ui-2xs text-foreground outline-none placeholder:text-muted-foreground focus:border-ring/55 focus:ring-2 focus:ring-ring/15"
              ></textarea>
              <p class="text-ui-3xs text-muted-foreground">
                Prepended to the system prompt on every turn.
              </p>
            </div>

            <!-- Reset params -->
            <button
              type="button"
              class="self-start font-mono text-ui-3xs text-muted-foreground hover:text-foreground"
              onclick={resetChatParams}>Reset to defaults</button
            >

            <!-- Divider -->
            <div class="border-t border-border/30"></div>

            <!-- Clear all history -->
            <div class="flex flex-col gap-1.5">
              <p class="text-ui-xs font-medium text-foreground">Chat history</p>
              <p class="text-ui-3xs text-muted-foreground">
                Permanently delete all saved conversations for this connection.
              </p>
              <button
                type="button"
                class="mt-1 flex h-8 w-full items-center justify-center gap-1.5 rounded-md border border-destructive/40 bg-destructive/8 text-ui-xs text-destructive transition-colors hover:bg-destructive/15"
                onclick={async () => {
                  if (
                    !confirm("Delete all conversations? This cannot be undone.")
                  )
                    return;
                  const { clearConversations } = await import(
                    "$lib/stores/conversations.js"
                  );
                  await clearConversations(chatMatch ?? (connectionId || undefined));
                  convList = [];
                  await newConversation();
                }}
              >
                <Trash2 class="size-3.5 shrink-0" />
                Clear all history
              </button>
            </div>
          </div>

          <!-- ── Skills tab ── -->
        {:else if settingsTab === "skills"}
          <div class="flex flex-col gap-3 p-4">
            <!-- What skills are, then the two ways to add one, on their own row
                 so neither is ever cut off in a narrow panel. -->
            <div class="flex flex-col gap-2.5">
              <div>
                <p class="text-ui-sm font-medium text-foreground">Skills</p>
                <p class="mt-0.5 text-ui-2xs leading-relaxed text-muted-foreground">
                  Know-how the assistant reads before it answers. Built-in skills load on their own when a question needs them; your skills apply to every request.
                </p>
              </div>
              <div class="flex items-center gap-1.5">
                <Button type="button" size="sm" class="h-7 gap-1 px-2.5 text-ui-xs" onclick={() => (newSkillOpen = !newSkillOpen)}>
                  <Plus class="size-3.5 shrink-0" /> New skill
                </Button>
                <label class="field-surface inline-flex h-7 cursor-pointer items-center gap-1 px-2.5 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
                  <Upload class="size-3.5 shrink-0" />
                  Upload .md
                  <input type="file" accept=".md,text/markdown,text/plain" multiple class="sr-only" onchange={handleSkillFileUpload} />
                </label>
              </div>
            </div>

            <!-- New skill form -->
            {#if newSkillOpen}
              <div
                class="flex flex-col gap-2 rounded-lg border border-border/50 bg-background/60 p-3"
              >
                <p class="text-ui-xs font-medium">New skill</p>
                <div class="flex flex-col gap-1">
                  <label
                    for="skill-name"
                    class="text-ui-3xs text-muted-foreground">Name *</label
                  >
                  <Input
                    id="skill-name"
                    class="h-7 text-ui-xs"
                    placeholder="e.g. postgres-patterns"
                    bind:value={newSkillName}
                  />
                </div>
                <div class="flex flex-col gap-1">
                  <label
                    for="skill-desc"
                    class="text-ui-3xs text-muted-foreground"
                    >Description</label
                  >
                  <Input
                    id="skill-desc"
                    class="h-7 text-ui-xs"
                    placeholder="When to apply…"
                    bind:value={newSkillDesc}
                  />
                </div>
                <div class="flex flex-col gap-1">
                  <label
                    for="skill-content"
                    class="text-ui-3xs text-muted-foreground"
                    >Content (Markdown) *</label
                  >
                  <textarea
                    id="skill-content"
                    class="min-h-[90px] w-full resize-y rounded-lg border-2 border-border bg-background px-2.5 py-2 font-mono text-ui-xs leading-relaxed text-foreground outline-none focus:border-ring/55 focus:ring-2 focus:ring-ring/15 placeholder:text-muted-foreground"
                    placeholder="# My Skill&#10;&#10;Guidelines in Markdown..."
                    bind:value={newSkillContent}
                  ></textarea>
                </div>
                <div class="flex justify-end gap-1.5">
                  <button
                    type="button"
                    class= "field-surface inline-flex h-7 items-center px-3 text-ui-xs text-muted-foreground hover:bg-accent"
                    onclick={() => (newSkillOpen = false)}>Cancel</button
                  >
                  <Button
                    size="sm"
                    class="h-7 px-3 text-ui-xs"
                    disabled={!newSkillName.trim() || !newSkillContent.trim()}
                    onclick={createSkill}>Save</Button
                  >
                </div>
              </div>
            {/if}

            <!-- Built-in: what each one covers and when it loads. -->
            <div class="flex flex-col gap-1.5">
              <p class="text-ui-2xs font-medium text-muted-foreground">Built-in</p>
              <ul class="overflow-hidden rounded-lg border border-border/50">
                {#each [
                  { name: "PostgreSQL", desc: "Schema design, indexing and pitfalls", when: "design, indexes, performance" },
                  { name: "MySQL", desc: "Schema design, indexing and pitfalls", when: "design, indexes, performance" },
                  { name: "SQLite", desc: "Schema design, limits and pragmas", when: "design, indexes, performance" },
                  { name: "Operations", desc: "Sizes, WAL, settings, connections, locks, vacuum", when: "sizes, config, WAL, locks" },
                  { name: "Diagrams", desc: "ER, flow, sequence, state and class diagrams", when: "diagrams, ERDs" },
                  { name: "Charts", desc: "Interactive ECharts from query results", when: "charts, graphs, plots" },
                ] as b, k (b.name)}
                  <li class={cn("flex items-start gap-2.5 px-3 py-2", k > 0 && "border-t border-border/40")}>
                    <span class="mt-1.5 size-1.5 shrink-0 rounded-full bg-primary/70" aria-hidden="true"></span>
                    <div class="min-w-0 flex-1">
                      <p class="text-ui-xs font-medium text-foreground">{b.name}</p>
                      <p class="truncate text-ui-2xs text-muted-foreground">{b.desc}</p>
                    </div>
                    <span class="mt-0.5 shrink-0 text-ui-3xs text-muted-foreground/80" title="Loads when a question is about {b.when}">auto</span>
                  </li>
                {/each}
              </ul>
            </div>

            <!-- Custom skills -->
            {#if skills.length === 0}
              <div class="flex flex-col gap-1.5">
                <p class="text-ui-2xs font-medium text-muted-foreground">Yours</p>
                <div class="flex items-start gap-2.5 rounded-lg border border-dashed border-border/50 px-3 py-3">
                  <BookOpen class="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                  <p class="text-ui-2xs leading-relaxed text-muted-foreground">
                    None yet. Add your team's conventions - naming rules, which tables matter, how to read a status column - and the assistant follows them in every answer.
                  </p>
                </div>
              </div>
            {:else}
              <div class="flex flex-col gap-1">
                <p
                  class="text-ui-2xs font-medium text-muted-foreground"
                >
                  Yours ({skills.length})
                </p>
                {#each skills as skill (skill.id)}
                  <div
                    class="flex items-start gap-2 rounded-lg border border-border/40 bg-background/50 px-3 py-2.5"
                  >
                    <BookOpen
                      class="mt-0.5 size-3.5 shrink-0 text-primary"
                    />
                    <div class="min-w-0 flex-1">
                      <p class="truncate text-ui-xs font-medium">
                        {skill.name}
                      </p>
                      {#if skill.description}
                        <p
                          class="mt-0.5 line-clamp-2 text-ui-3xs leading-relaxed text-muted-foreground"
                        >
                          {skill.description}
                        </p>
                      {/if}
                    </div>
                    <button
                      type="button"
                      class="hit-area mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                      onclick={() => removeSkill(skill.id)}
                      title="Remove"
                    >
                      <X class="size-3 shrink-0" />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <!-- ── Context tab ── -->
        {:else}
          <div class="flex flex-col gap-4 p-4">
            <!-- Context gauge -->
            <div class="flex flex-col gap-2">
              <div class="flex items-center justify-between">
                <span class="text-ui-xs font-medium">Context window</span>
                <span
                  class="font-mono text-ui-3xs tabular-nums text-muted-foreground"
                  >{tokEst(contextStats.totalChars)} / {tokEst(
                    contextStats.maxChars,
                  )}</span
                >
              </div>
              <div
                class="h-1.5 w-full overflow-hidden rounded-full bg-muted/60"
              >
                <div
                  class={cn(
                    "h-full rounded-full transition-[width] duration-200",
                    contextStats.pct >= 90
                      ? "bg-destructive"
                      : contextStats.pct >= 70
                        ? "bg-warning"
                        : "bg-primary",
                  )}
                  style="width: {Math.min(contextStats.pct, 100)}%"
                ></div>
              </div>
              <p class="text-ui-3xs text-muted-foreground">
                {contextStats.pct}% used · auto-compresses at 15k tokens
              </p>
            </div>

            <!-- Stats 2×2 grid -->
            <div class="grid grid-cols-2 gap-2">
              {#each [{ label: "Turns", value: String(contextStats.messages) }, { label: "History", value: tokEst(contextStats.historyChars) }, { label: "System", value: tokEst(contextStats.promptChars) }, { label: "Tools", value: tokEst(contextStats.toolsChars) }, { label: "Total", value: tokEst(contextStats.totalChars) }] as stat}
                <div
                  class="flex flex-col gap-0.5 rounded-lg border border-border/40 bg-background/60 px-3 py-2.5"
                >
                  <span
                    class="font-mono text-ui font-semibold tabular-nums text-foreground"
                    >{stat.value}</span
                  >
                  <span class="text-ui-3xs text-muted-foreground"
                    >{stat.label}</span
                  >
                </div>
              {/each}
            </div>

            <!-- Info pills -->
            <div
              class="flex flex-col gap-1.5 rounded-lg border border-border/30 bg-muted/15 p-3"
            >
              {#each [{ color: "bg-primary/50", text: "Full history re-sent each turn for context." }, { color: "bg-primary/50", text: "Compresses at 30k, keeps last 10 turns." }, { color: "bg-primary/50", text: "Only schema for mentioned tables is injected." }, { color: "bg-warning/60", text: "Failed tool calls blocked after 2 retries." }] as item}
                <div
                  class="flex items-start gap-2 text-ui-3xs text-muted-foreground"
                >
                  <span
                    class="mt-1.5 size-1.5 shrink-0 rounded-full {item.color}"
                  ></span>
                  {item.text}
                </div>
              {/each}
            </div>

            <!-- Clear button -->
            <button
              type="button"
              class="flex h-8 w-full items-center justify-center gap-1.5 rounded-md border border-destructive/25 text-ui-xs text-destructive transition-colors hover:bg-destructive/6 hover:border-destructive/40 hover:text-destructive"
              onclick={() => {
                apiHistory = [];
                rawApiHistory = [];
                fetchedSchemas = {};
                error = "";
              }}
            >
              <Trash2 class="size-3.5 shrink-0" />
              Clear history & cache
            </button>
          </div>
        {/if}
      </div>
    </aside>
  {/if}

  <!-- ── Right-click context menu ─────────────────────────────────────── -->
  {#if contextMenu}
    {@const menu = contextMenu}
    <!-- invisible backdrop to close on outside click -->
    <div
      role="presentation"
      class="fixed inset-0 z-[200]"
      onclick={closeContextMenu}
      oncontextmenu={closeContextMenu}
    ></div>
    <div
      class="fixed z-[201] min-w-[10rem] overflow-hidden rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim"
      style="left:{menu.x}px;top:{menu.y}px"
    >
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm hover:bg-accent hover:text-foreground"
        onclick={() => {
          void selectConversation(menu.id);
          closeContextMenu();
        }}
      >
        Open
      </button>
      <div class="my-1 h-px bg-border"></div>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-sm text-destructive hover:bg-destructive/10"
        onclick={() => void removeConversation(menu.id)}
      >
        <Trash2 class="size-3 shrink-0" /> Delete
      </button>
    </div>
  {/if}
</div>

<!-- ── Selection toolbar ─────────────────────────────────────────────────────
     Copy / quote whatever is selected in the transcript. Sits above the
     selection, clamped to the viewport so a selection near an edge can't push
     it off screen. mousedown is swallowed: letting it through would collapse
     the selection and unmount this bar before the click landed. -->
{#if selToolbar}
  <div
    class="fixed z-50 -translate-x-1/2 -translate-y-full pb-1.5"
    style="left:clamp(4.5rem, {selToolbar.x}px, calc(100vw - 4.5rem)); top:{selToolbar.y}px"
    onmousedown={(e) => e.preventDefault()}
    role="toolbar"
    tabindex="-1"
    aria-label="Selection actions"
  >
    <div
      class="flex items-center gap-0.5 rounded-md border border-border/60 bg-popover p-0.5 elevate-2-rim"
    >
      <button
        type="button"
        class="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground hover:bg-accent hover:text-foreground"
        onclick={() => {
          const text = selToolbar?.text ?? "";
          void copyWithFeedback("selection:msg", text);
          clearSelectionToolbar();
          toast.success("Selection copied");
        }}
      >
        <Copy class="size-3 shrink-0" />Copy
      </button>
      <button
        type="button"
        class="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground hover:bg-accent hover:text-foreground"
        onclick={() => quoteToInput(selToolbar?.text ?? "")}
      >
        <Reply class="size-3 shrink-0" />Reply
      </button>
    </div>
  </div>
{/if}

<!-- ── Diagram fullscreen modal ──────────────────────────────────────────── -->
{#if fullscreenChart}
  <!-- Chart fullscreen overlay -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="Chart fullscreen"
    class="fixed inset-0 z-[500] flex flex-col bg-background"
    onkeydown={(e) => {
      if (e.key === "Escape") closeChartFullscreen();
    }}
    use:focusTrap
  >
    <!-- Header -->
    <div
      class="window-overlay-head flex shrink-0 items-center gap-3 border-b border-border/60 bg-background px-4 py-2" data-tauri-drag-region
    >
      <span
        class="min-w-0 flex-1 truncate font-mono text-ui-sm font-semibold text-foreground/80"
        >{fullscreenChart.title}</span
      >
      <span class="font-mono text-ui-3xs capitalize text-muted-foreground"
        >{fullscreenChart.spec.type}</span
      >
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Download PNG"
          onclick={() =>
            void saveChartPng("[data-chart-fs] canvas", fullscreenChart?.title ?? "")}><ArrowDownToLine class="size-3.5 shrink-0" /></button
        >
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Close (Esc)"
          onclick={closeChartFullscreen}><X class="size-4 shrink-0" /></button
        >
      </div>
    </div>
    <!-- Chart body, fills remaining space; scrollZoom lets ECharts handle wheel natively -->
    <div class="min-h-0 flex-1" data-chart-fs>
      <AiChartRenderer
        spec={fullscreenChart.spec}
        noTitle={true}
        scrollZoom={true}
      />
    </div>
    <p
      class="shrink-0 border-t border-border/30 px-4 py-1.5 text-center font-mono text-ui-3xs text-muted-foreground"
    >
      Ctrl+scroll to zoom · drag to pan · double-click to reset
    </p>
  </div>
{/if}

{#if fullscreenDiagramCode}
  <!-- Backdrop -->
  <div
    role="dialog"
    aria-modal="true"
    aria-label="Diagram fullscreen"
    class="fixed inset-0 z-[500] flex flex-col bg-background"
    tabindex="-1"
    use:focusTrap
  >
    <!-- Header toolbar -->
    <div
      class="window-overlay-head flex shrink-0 items-center gap-2 border-b border-border bg-background px-4 py-2" data-tauri-drag-region
    >
      <span class="text-ui-xs font-medium text-muted-foreground">Diagram</span>
      <div class="ml-auto flex items-center gap-1">
        <!-- Zoom controls -->
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1 px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => dispatchDiagramEvent("diagram:zoomout")}
          title="Zoom out"
        >
          <ZoomOut class="size-3 shrink-0" />
        </button>
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1 px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => dispatchDiagramEvent("diagram:reset")}
          title="Reset zoom (double-click canvas)"
        >
          <RotateCcw class="size-3 shrink-0" />Reset
        </button>
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1 px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => dispatchDiagramEvent("diagram:zoomin")}
          title="Zoom in"
        >
          <ZoomIn class="size-3 shrink-0" />
        </button>
        <div class="mx-1 h-4 w-px bg-border"></div>
        <!-- Export -->
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1.5 px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => void exportDiagramSvg()}
          title="Export as SVG"
        >
          <ArrowDownToLine class="size-3 shrink-0" />SVG
        </button>
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1.5 px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => void exportDiagramPng()}
          title="Export as PNG"
        >
          <ArrowDownToLine class="size-3 shrink-0" />PNG
        </button>
        <div class="mx-1 h-4 w-px bg-border"></div>
        <!-- Close -->
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center gap-1.5 px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={closeDiagramFullscreen}
          title="Close (Esc)"
        >
          <Minimize2 class="size-3 shrink-0" />Exit
        </button>
      </div>
    </div>
    <!-- Canvas fills remaining height -->
    <div class="min-h-0 flex-1 overflow-hidden">
      <div
        bind:this={fullscreenCanvasEl}
        class="mermaid-canvas h-full w-full"
        use:mermaidInteractive
      >
        {@html processMermaidSvg(fullscreenDiagramCode)}
      </div>
    </div>
    <p
      class="shrink-0 border-t border-border/40 px-4 py-1.5 text-center text-ui-3xs text-muted-foreground"
    >
      Drag to pan · Ctrl+scroll to zoom · Double-click to reset
    </p>
  </div>
{/if}

<style>
  /* Thinking-indicator style - driven by data-thinking-style on <html> (see settings.js applySettings). */
  :global([data-thinking-style="shimmer"]) .agent-think-label {
    background: linear-gradient(
      90deg,
      var(--muted-foreground) 0%,
      var(--muted-foreground) 35%,
      var(--foreground) 50%,
      var(--muted-foreground) 65%,
      var(--muted-foreground) 100%
    );
    background-size: 220% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    color: transparent;
    animation: agent-think-shimmer 1.8s linear infinite;
  }
  :global([data-thinking-style="pulse"]) .agent-think-label {
    animation: agent-think-pulse 1.4s ease-in-out infinite;
  }
  /* static → no animation (default text) */
  @keyframes agent-think-shimmer {
    0% { background-position: 220% 0; }
    100% { background-position: -20% 0; }
  }
  @keyframes agent-think-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
  @media (prefers-reduced-motion: reduce) {
    :global([data-thinking-style]) .agent-think-label { animation: none; }
  }

  /* New turns ease in: a short rise and fade, so a reply, a fold or a result
     arrives instead of appearing. The app's motion setting switches it off. */
  @keyframes ai-enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  :global(.ai-enter) {
    animation: ai-enter 220ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }

  /* The composer and the user's own turns read at the transcript's size, so one
     conversation is set in one size rather than three. */
  .ai-composer-input,
  :global(.ai-user-bubble) {
    font-family: var(--font-reading, var(--font-sans));
    font-size: var(--ai-chat-font-size, 0.9375rem);
    line-height: 1.6;
    letter-spacing: -0.011em;
  }

  :global(.prose-ai) {
    /* Follow the app's font setting through its reading face (the sans, or a
       proportional face under the Mono preset). This used to hardcode the
       Inter stack, so picking another font changed the composer and the
       user's turns but left every response in Inter: one conversation in two
       typefaces. */
    font-family: var(--font-reading, var(--font-sans));
    font-size: var(--ai-chat-font-size, 0.9375rem);
    line-height: 1.65;
    color: var(--foreground);
    word-break: break-word;
    letter-spacing: -0.011em;
    /* Inherit font-smoothing - do NOT override to 'antialiased' here
       (breaks FreeType hinting on Linux/WebKitGTK). */
  }
  :global(.prose-ai > *:first-child) {
    margin-top: 0;
  }
  :global(.prose-ai > *:last-child) {
    margin-bottom: 0;
  }
  :global(.prose-ai p) {
    margin: 0.45rem 0;
  }
  :global(.prose-ai strong) {
    font-weight: 600;
  }
  :global(.prose-ai em) {
    font-style: italic;
  }
  :global(.prose-ai h1, .prose-ai h2, .prose-ai h3, .prose-ai h4) {
    font-weight: 650;
    line-height: 1.3;
    margin: 0.85rem 0 0.3rem;
    color: var(--foreground);
  }
  /* Relative to the chat size, so one setting moves the whole transcript. */
  :global(.prose-ai h1) {
    font-size: 1.2em;
  }
  :global(.prose-ai h2) {
    font-size: 1.1em;
  }
  :global(.prose-ai h3) {
    font-size: 1em;
  }
  :global(.prose-ai ul) {
    padding-inline-start: 1.35rem;
    list-style-type: disc;
    margin: 0.4rem 0;
  }
  :global(.prose-ai ol) {
    padding-inline-start: 1.35rem;
    list-style-type: decimal;
    margin: 0.4rem 0;
  }
  :global(.prose-ai li) {
    margin: 0.2rem 0;
  }
  /* Bullets and numbers mark the structure; at full text colour they read as
     loud as the words they introduce. */
  :global(.prose-ai li::marker) {
    color: var(--muted-foreground);
  }
  :global(.prose-ai code) {
    /* The app's mono, not a hardcoded family: code and tables read in the same
       face as the grid and the SQL editor whatever font preset is on. */
    font-family: var(--font-mono);
    font-size: 0.85em;
    /* A tint of the text colour, no border and the text's own weight: a table
       name in a sentence is a word set in mono, not a control. The bordered,
       500-weight chip read as a key cap, heavier than the bold around it.
       Foreground-based so it shows on every theme's background, where --muted
       sits too close to some of them to read without the border. */
    background: color-mix(in oklch, var(--foreground) 7%, transparent);
    border-radius: 0.3em;
    padding: 0.1em 0.35em;
    color: var(--foreground);
    /* Long inline code wraps like the sentence it is in. It used to be
       nowrap, which pushed a backticked query past the message's edge; each
       wrapped piece keeps its own padding and rounded ends. */
    -webkit-box-decoration-break: clone;
    box-decoration-break: clone;
  }
  :global(.prose-ai a code) {
    color: inherit;
  }
  /* Keys, which the markdown renderer makes of a backticked shortcut. Sized in
     em so they follow the chat's text size (the app-wide kbd is a fixed 20px
     cap for menus), on the text baseline rather than centred, and selectable
     so a copied sentence keeps its shortcut. The bottom edge is what tells a
     key from the flat code tint beside it. */
  :global(.prose-ai kbd) {
    display: inline-block;
    height: auto;
    min-width: 1.6em;
    padding: 0.05em 0.4em;
    font-size: 0.8em;
    line-height: 1.4;
    text-align: center;
    vertical-align: baseline;
    color: var(--foreground);
    border-radius: 0.3em;
    box-shadow: inset 0 -1px 0 var(--border);
    user-select: text;
  }
  :global(.prose-ai .ai-kbd-chord) {
    white-space: nowrap;
    color: var(--muted-foreground);
  }
  :global(.prose-ai .ai-kbd-chord kbd) {
    margin-inline: 0.125em;
  }
  :global(.prose-ai pre:not(.shiki)) {
    background: var(--muted);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.75rem;
    overflow-x: auto;
    margin: 0.5rem 0;
  }
  :global(.prose-ai pre:not(.shiki) code) {
    background: none;
    border: none;
    padding: 0;
    font-size: var(--ai-code-font-size, 0.825rem);
  }
  :global(.prose-ai pre.shiki) {
    margin: 0.5rem 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow-x: auto;
    background: var(--editor-surface) !important;
  }
  :global(.prose-ai pre.shiki code) {
    font-family: var(--font-mono);
    font-size: var(--ai-code-font-size, 0.825rem);
    line-height: 1.6;
  }
  :global(.prose-ai-loading pre.shiki) {
    opacity: 0.7;
  }
  /* The wrapper scrolls, the table doesn't. This used to set `overflow-x: auto`
     and then `overflow: hidden` on the same element - the shorthand won, so a
     table wider than the message was clipped with no way to reach the rest of
     it. Rounding lives on the wrapper for the same reason. */
  /* The inner element scrolls, the outer one anchors the copy button. This used
     to be one element doing both, which sent the button sliding into the middle
     of the table on horizontal scroll. */
  :global(.ai-table-scroll) {
    overflow: auto;
    max-height: 26rem;
    -webkit-overflow-scrolling: touch;
    overscroll-behavior: contain;
    border: 1px solid var(--border);
    border-radius: 8px;
    margin: 0.6rem 0;
    /* Hug the table: a three-row answer should not stretch the full column. */
    width: fit-content;
    max-width: 100%;
  }
  :global(.prose-ai table) {
    border-collapse: collapse;
    width: max-content;
    font-size: 0.875em;
  }
  :global(.prose-ai th) {
    position: sticky;
    top: 0;
    z-index: 1;
    border-bottom: 1px solid var(--border);
    border-right: 1px solid color-mix(in oklch, var(--border) 60%, transparent);
    padding: 0.35rem 0.85rem;
    background: color-mix(in oklch, var(--muted) 50%, transparent);
    font-weight: 500;
    text-align: left;
    white-space: nowrap;
    color: var(--muted-foreground);
  }
  :global(.prose-ai th:last-child) {
    border-right: none;
  }
  :global(.prose-ai td) {
    border-bottom: 1px solid color-mix(in oklch, var(--border) 45%, transparent);
    border-right: 1px solid color-mix(in oklch, var(--border) 40%, transparent);
    padding: 0.3rem 0.85rem;
    font-family: var(--font-mono);
    font-size: 1em;
    white-space: nowrap;
    color: var(--foreground);
  }
  :global(.prose-ai td:last-child) {
    border-right: none;
  }
  :global(.prose-ai tr:last-child td) {
    border-bottom: none;
  }
  :global(.prose-ai tr:hover td) {
    background: color-mix(in oklch, var(--muted) 35%, transparent);
  }
  :global(.prose-ai blockquote) {
    border-inline-start: 2px solid var(--border);
    padding-inline-start: 0.75rem;
    color: var(--muted-foreground);
    margin: 0.35rem 0;
  }
  :global(.prose-ai a) {
    color: var(--link);
    text-decoration: underline;
    text-underline-offset: 2px;
    text-decoration-color: color-mix(in oklch, var(--link) 45%, transparent);
  }
  :global(.prose-ai a:hover) {
    color: var(--link-hover);
    text-decoration-color: var(--link);
  }
  :global(.prose-ai hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 0.75rem 0;
  }

  :global(.mermaid-canvas) {
    cursor: grab;
    overflow: hidden;
    position: relative;
    padding: 1rem;
    /* background is set directly by the mermaidInteractive action */
    background: #ffffff;
    /* Block app :root tokens that collide with beautiful-mermaid variable names */
    --muted: unset;
    --accent: unset;
    --border: unset;
  }
  :global(.mermaid-canvas.is-dragging) {
    cursor: grabbing;
  }

  /* Images are never rendered - the custom marked renderer outputs link chips instead.
     This rule is a safety net for any stray <img> that might appear from other sources. */
  :global(.prose-ai img) { display: none !important; }

  /* ── Image-link chips (replaces inline images) ───────────────────────────── */
  :global(.prose-ai-img-link) {
    display: inline-flex;
    align-items: center;
    gap: 0.25em;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.75em;
    padding: 0.1em 0.45em;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: color-mix(in oklch, var(--muted) 60%, transparent);
    color: var(--muted-foreground);
    text-decoration: none;
    vertical-align: middle;
  }
  :global(.prose-ai-img-link:hover) {
    color: var(--foreground);
    background: var(--muted);
  }

  :global(.mermaid-canvas svg) {
    display: block;
    /* Respect the SVG's natural size; shrink only if wider than the container */
    max-width: 100%;
    width: auto;
    height: auto;
    transform-origin: 0 0;
    user-select: none;
    -webkit-user-select: none;
  }
</style>
