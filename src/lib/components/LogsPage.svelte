<script>
  import { onMount } from "svelte";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Table2 from "@lucide/svelte/icons/table-2";
  import Terminal from "@lucide/svelte/icons/terminal";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Trash from "@lucide/svelte/icons/trash";
  import Plus from "@lucide/svelte/icons/plus";
  import FileDown from "@lucide/svelte/icons/file-down";
  import Unplug from "@lucide/svelte/icons/unplug";
  import Database from "@lucide/svelte/icons/database";
  import Search from "@lucide/svelte/icons/search";
  import Bot from "@lucide/svelte/icons/bot";
  import X from "@lucide/svelte/icons/x";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CheckCircle2 from "@lucide/svelte/icons/check-circle-2";
  import XCircle from "@lucide/svelte/icons/x-circle";
  import { cn } from "$lib/utils.js";
  import {
    subscribeActivityLog,
    clearActivityLog,
  } from "$lib/stores/activity-log.js";

  let { active = false } = $props();

  /** @type {import('$lib/stores/activity-log.js').ActivityEntry[]} */
  let entries = $state([]);
  let search = $state("");
  let searchEl = $state(/** @type {HTMLInputElement | null} */ (null));
  /** @type {string} */
  let typeFilter = $state("all");
  /** @type {string | null} */
  let expanded = $state(null);

  onMount(() =>
    subscribeActivityLog((log) => {
      entries = log;
    }),
  );

  const FILTERS = [
    { id: "all", label: "All" },
    { id: "sql_exec", label: "SQL" },
    { id: "table_open", label: "Tables" },
    { id: "row_save", label: "Edits" },
    { id: "row_delete", label: "Deletes" },
    { id: "row_insert", label: "Inserts" },
    { id: "connect", label: "Connections" },
    { id: "mcp", label: "MCP" },
    { id: "error", label: "Errors" },
  ];

  const TYPE_LABEL = /** @type {Record<string,string>} */ ({
    connect: "Connect",
    disconnect: "Disconnect",
    table_open: "Table",
    row_fetch: "Fetch",
    row_save: "Save",
    row_delete: "Delete",
    row_insert: "Insert",
    sql_exec: "SQL",
    mcp: "MCP",
    export: "Export",
    schema_change: "Schema",
    filter: "Filter",
    error: "Error",
  });

  const filtered = $derived.by(() => {
    let list = entries;
    if (typeFilter === "error") list = list.filter((e) => !e.success);
    else if (typeFilter === "connect")
      list = list.filter(
        (e) => e.type === "connect" || e.type === "disconnect",
      );
    else if (typeFilter !== "all")
      list = list.filter((e) => e.type === typeFilter);
    const q = search.trim().toLowerCase();
    if (q)
      list = list.filter(
        (e) =>
          e.title.toLowerCase().includes(q) ||
          e.detail?.toLowerCase().includes(q) ||
          e.table?.toLowerCase().includes(q) ||
          e.error?.toLowerCase().includes(q),
      );
    return list;
  });

  const errorCount = $derived(entries.filter((e) => !e.success).length);
  const timedEntries = $derived(entries.filter((e) => e.durationMs != null));
  const avgMs = $derived(
    timedEntries.length > 0
      ? Math.round(
          timedEntries.reduce((s, e) => s + (e.durationMs ?? 0), 0) /
            timedEntries.length,
        )
      : null,
  );

  /** @param {import('$lib/stores/activity-log.js').ActivityEntry} e */
  function icon(e) {
    if (e.type === "connect" || e.type === "disconnect") return Unplug;
    if (e.type === "table_open" || e.type === "row_fetch") return Table2;
    if (e.type === "row_save") return Pencil;
    if (e.type === "row_delete") return Trash;
    if (e.type === "row_insert") return Plus;
    if (e.type === "sql_exec") return Terminal;
    if (e.type === "mcp") return Bot;
    if (e.type === "export") return FileDown;
    if (e.type === "schema_change") return Database;
    return Terminal;
  }

  /** @param {import('$lib/stores/activity-log.js').ActivityEntry} e */
  function chipClass(e) {
    if (!e.success) return "bg-destructive/10 text-destructive";
    if (e.type === "connect") return "bg-primary/10 text-primary";
    if (e.type === "disconnect") return "bg-muted text-muted-foreground";
    if (e.type === "row_delete") return "bg-destructive/10 text-destructive";
    if (e.type === "row_insert") return "bg-primary/10 text-primary";
    if (e.type === "row_save") return "bg-muted text-foreground/70";
    if (e.type === "sql_exec") return "bg-muted text-muted-foreground";
    return "bg-muted text-muted-foreground";
  }

  /** @param {number} ts */
  function fmtTime(ts) {
    return new Date(ts).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (!active) return;
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === "r") {
      e.preventDefault();
      search = "";
      typeFilter = "all";
      expanded = null;
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === "f") {
      e.preventDefault();
      searchEl?.focus();
      searchEl?.select();
    }
  }}
/>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
  <!-- Header -->
  <div
    class="studio-chrome flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel px-3"
  >
    <ScrollText class="size-3.5 shrink-0 text-muted-foreground" />
    <span class="font-mono text-ui-sm font-medium">Activity log</span>
    {#if entries.length > 0}
      <span
        class="rounded-full bg-muted px-2 py-px font-mono text-ui-2xs text-muted-foreground"
        >{entries.length}</span
      >
    {/if}
    <div class="flex-1"></div>
    {#if errorCount > 0}
      <span
        class="rounded-full bg-destructive/10 px-2 py-px font-mono text-ui-2xs text-destructive"
        >{errorCount} error{errorCount === 1 ? "" : "s"}</span
      >
    {/if}
    <button
      type="button"
      class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-destructive disabled:opacity-30"
      title="Clear all"
      onclick={() => clearActivityLog()}
      disabled={entries.length === 0}
    >
      <Trash2 class="size-3.5 shrink-0" />
    </button>
  </div>

  <!-- Filter toolbar -->
  <div
    class="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 bg-panel px-3 py-2"
  >
    <!-- Search -->
    <div class="relative min-w-0 flex-1">
      <Search
        class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
      />
      <input
        type="text"
        placeholder="Search logs…"
        class= "field-surface h-7 w-full bg-background/40 pl-7 pr-2.5 font-mono text-ui-xs text-foreground outline-none placeholder:text-muted-foreground transition-colors hover:bg-background/60"
        bind:this={searchEl}
        bind:value={search}
      />
      {#if search}
        <button
          type="button"
          class="absolute top-1/2 right-1.5 -translate-y-1/2 inline-flex size-4 items-center justify-center rounded-md text-muted-foreground hover:text-foreground"
          onclick={() => {
            search = "";
          }}><X class="size-3 shrink-0" /></button
        >
      {/if}
    </div>
    <!-- Type filter chips -->
    <div
    class= "field-surface inline-flex h-7 shrink-0 items-center bg-muted/40 p-0.5 ring-1 ring-inset ring-border/40"
    >
      {#each FILTERS as f (f.id)}
        <button
          type="button"
          class={cn(
            "inline-flex h-6 items-center rounded-md px-2 font-mono text-ui-2xs transition-colors",
            typeFilter === f.id
              ? "bg-card text-foreground shadow-sm ring-1 ring-border/50"
              : "text-muted-foreground hover:text-foreground",
          )}
          onclick={() => {
            typeFilter = f.id;
          }}>{f.label}</button
        >
      {/each}
    </div>
  </div>

  <!-- Log list -->
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto p-1.5 [will-change:transform]">
    {#if filtered.length === 0}
      <div
        class="flex h-full flex-col items-center justify-center gap-3 text-center"
      >
        <ScrollText class="size-10 shrink-0 text-muted-foreground" />
        <p class="font-mono text-ui text-muted-foreground">
          {entries.length === 0 ? "No activity yet" : "No entries match"}
        </p>
        <p class="text-ui-xs text-muted-foreground">
          {entries.length === 0
            ? "Operations will appear here as you use the app"
            : "Try a different filter"}
        </p>
      </div>
    {:else}
      {#each filtered as entry (entry.id)}
        {@const Icon = icon(entry)}
        {@const isExpanded = expanded === entry.id}
        {@const hasDetail = !!(entry.detail || entry.error)}
        <div
          class={cn(
            "group relative mb-px rounded-md transition-colors",
            entry.success
              ? "hover:bg-accent/25"
              : "bg-destructive/5 hover:bg-destructive/10",
            isExpanded && "bg-accent/15",
          )}
        >
          <button
            type="button"
            class="flex w-full min-w-0 items-center gap-2 px-2 py-2 text-left"
            onclick={() => {
              if (hasDetail) expanded = isExpanded ? null : entry.id;
            }}
            disabled={!hasDetail}
          >
            <!-- Status icon -->
            {#if !entry.success}
              <XCircle class="size-3.5 shrink-0 text-destructive" />
            {:else}
              <CheckCircle2
                class="size-3.5 shrink-0 text-muted-foreground"
              />
            {/if}

            <!-- Type chip -->
            <span
              class={cn(
                "inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-px font-mono text-ui-xs font-medium lowercase",
                chipClass(entry),
              )}
            >
              <Icon class="size-3 shrink-0" />
              {TYPE_LABEL[entry.type] ?? entry.type}
            </span>

            <!-- Title -->
            <span
              class="min-w-0 flex-1 truncate font-mono text-ui-xs text-foreground/80"
              >{entry.title}</span
            >

            <!-- Table badge -->
            {#if entry.table}
              <span
                class="shrink-0 rounded bg-muted px-1.5 py-px font-mono text-ui-2xs text-muted-foreground"
              >
                {entry.schema ? `${entry.schema}.` : ""}{entry.table}
              </span>
            {/if}

            <!-- Row count -->
            {#if entry.rowCount != null}
              <span
                class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground"
                >{entry.rowCount.toLocaleString()}r</span
              >
            {/if}

            <!-- Duration -->
            {#if entry.durationMs != null}
              <span
                class="w-12 shrink-0 text-right font-mono text-ui-2xs tabular-nums text-muted-foreground"
                >{entry.durationMs}ms</span
              >
            {/if}

            <!-- Time -->
            <span
              class="w-[58px] shrink-0 text-right font-mono text-ui-2xs tabular-nums text-muted-foreground"
              >{fmtTime(entry.timestamp)}</span
            >

            <!-- Expand chevron -->
            {#if hasDetail}
              <ChevronRight
                class={cn(
                  "size-3 shrink-0 text-muted-foreground transition-transform",
                  isExpanded && "rotate-90",
                )}
              />
            {:else}
              <span class="size-3 shrink-0"></span>
            {/if}
          </button>

          <!-- Expanded detail -->
          {#if isExpanded}
            <div class="mx-2 mb-2 rounded-md border border-border/50 bg-card">
              <div
                class="flex items-center gap-3 border-b border-border/40 px-3 py-1.5 font-mono text-ui-2xs text-muted-foreground"
              >
                <span>{new Date(entry.timestamp).toLocaleString()}</span>
                {#if entry.durationMs != null}<span>{entry.durationMs}ms</span
                  >{/if}
                {#if entry.schema || entry.table}<span
                    >{[entry.schema, entry.table]
                      .filter(Boolean)
                      .join(".")}</span
                  >{/if}
              </div>
              {#if entry.detail}
                <pre
                  class="app-scroll max-h-40 overflow-auto px-3 py-2.5 font-mono text-ui-xs leading-relaxed text-foreground/90 [white-space:pre-wrap]">{entry.detail}</pre>
              {/if}
              {#if entry.error}
                <pre
                  class="app-scroll max-h-28 overflow-auto px-3 py-2.5 font-mono text-ui-xs leading-relaxed text-destructive [white-space:pre-wrap]">{entry.error}</pre>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <!-- Status bar -->
  {#if entries.length > 0}
    <div
      class="flex shrink-0 items-center gap-3 border-t border-border/40 bg-panel px-3 py-1"
    >
      <span class="font-mono text-ui-2xs text-muted-foreground tabular-nums"
        >{filtered.length} of {entries.length} events</span
      >
      {#if avgMs !== null}
        <span class="font-mono text-ui-2xs text-muted-foreground"
          >avg {avgMs}ms</span
        >
      {/if}
    </div>
  {/if}
</div>
