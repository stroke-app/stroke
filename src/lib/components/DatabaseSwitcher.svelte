<script>
  import { tick } from "svelte";
  import { executeSql } from "$lib/api.js";
  import Database from "@lucide/svelte/icons/database";
  import HardDrive from "@lucide/svelte/icons/hard-drive";
  import Check from "@lucide/svelte/icons/check";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import Search from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import { cn } from "$lib/utils.js";

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
    onswitchtodb = /** @type {(dbName: string) => void} */ (() => {}),
  } = $props();

  let open = $state(false);
  /** @type {string[]} */
  let databases = $state([]);
  let loading = $state(false);
  let error = $state("");
  let search = $state("");
  /** @type {HTMLButtonElement | null} */
  let triggerEl = $state(null);
  /** @type {HTMLDivElement | null} */
  let panelEl = $state(null);

  const currentDb = $derived(
    connection?.database ?? connection?.filePath ?? "",
  );
  const isPostgres = $derived(
    connection?.type === "postgres" || connection?.type === "mysql",
  );

  const filtered = $derived(
    search.trim()
      ? databases.filter((d) => d.toLowerCase().includes(search.toLowerCase()))
      : databases,
  );

  async function fetchDatabases() {
    if (!isPostgres) return;
    loading = true;
    error = "";
    try {
      const result = await executeSql(
        `SELECT datname FROM pg_catalog.pg_database WHERE datistemplate = false ORDER BY datname`,
      );
      databases = (result?.rows ?? []).map((r) => String(r[0]));
    } catch (e) {
      error = String(e);
      databases = [];
    } finally {
      loading = false;
    }
  }

  async function openPanel() {
    open = true;
    search = "";
    if (databases.length === 0) await fetchDatabases();
    await tick();
    panelEl?.querySelector("input")?.focus();
  }

  function close() {
    open = false;
    search = "";
  }

  function switchTo(db) {
    if (db === currentDb) {
      close();
      return;
    }
    close();
    onswitchtodb(db);
  }

  $effect(() => {
    if (!open) return;
    function onDown(/** @type {PointerEvent} */ e) {
      if (
        triggerEl &&
        !triggerEl.contains(/** @type {Node} */ (e.target)) &&
        panelEl &&
        !panelEl.contains(/** @type {Node} */ (e.target))
      )
        close();
    }
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  });
</script>

<div class="relative">
  <!-- Trigger -->
  <button
    bind:this={triggerEl}
    type="button"
    class={cn(
      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
      open
        ? "bg-accent/35 text-foreground"
        : "text-muted-foreground hover:bg-accent/25 hover:text-foreground",
    )}
    onclick={() => (open ? close() : openPanel())}
    disabled={!isPostgres && connection?.type !== "sqlite"}
  >
    {#if connection?.type === "sqlite"}
      <HardDrive class="size-3.5 shrink-0 opacity-50" />
    {:else}
      <Database class="size-3.5 shrink-0 opacity-50" />
    {/if}
    <span class="min-w-0 flex-1 truncate font-mono text-ui-xs"
      >{currentDb || connection?.name || "No database"}</span
    >
    {#if isPostgres}
      <ChevronUp
        class={cn(
          "size-3 shrink-0 opacity-40 transition-transform",
          !open && "rotate-180",
        )}
      />
    {/if}
  </button>

  <!-- Dropdown -->
  {#if open}
    <div
      bind:this={panelEl}
      class="absolute bottom-full left-0 right-0 z-50 mb-1 flex flex-col overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim"
      style="max-height: min(300px,55vh)"
      role="menu"
      tabindex="-1"
      onkeydown={(e) => {
        if (e.key === "Escape") close();
      }}
    >
      <!-- Search (only when >5 databases) -->
      {#if databases.length > 5}
        <div class="relative shrink-0 border-b border-border/40 px-2 py-1.5">
          <Search
            class="pointer-events-none absolute top-1/2 left-4 size-3 -translate-y-1/2 text-muted-foreground"
          />
          <input
            type="text"
            placeholder="Filter…"
            class= "field-surface h-6 w-full bg-background/40 pl-7 pr-2 font-mono text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
            bind:value={search}
          />
        </div>
      {/if}

      <div class="db-list-scroll app-scroll min-h-0 flex-1 overflow-y-auto p-1">
        {#if loading}
          <div
            class="loading-reveal flex items-center justify-center gap-2 py-6 text-muted-foreground"
          >
            <RefreshCw class="size-3.5 shrink-0 animate-spin" /><span
              class="font-mono text-ui-xs">Loading…</span
            >
          </div>
        {:else if error}
          <div
            class="px-2 py-4 text-center font-mono text-ui-xs text-destructive"
          >
            {error}
          </div>
        {:else if filtered.length === 0}
          <div
            class="px-2 py-4 text-center font-mono text-ui-xs text-muted-foreground"
          >
            {search ? "No match" : "No databases found"}
          </div>
        {:else}
          {#each filtered as db (db)}
            {@const isCurrent = db === currentDb}
            <button
              type="button"
              class={cn(
                "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
                isCurrent
                  ? "bg-accent/15 text-foreground"
                  : "text-foreground/80 hover:bg-accent/25 hover:text-foreground",
              )}
              onclick={() => switchTo(db)}
            >
              <Database
                class={cn(
                  "size-3.5 shrink-0",
                  isCurrent ? "text-primary" : "text-muted-foreground",
                )}
              />
              <span class="min-w-0 flex-1 truncate font-mono text-ui-xs"
                >{db}</span
              >
              {#if isCurrent}
                <Check class="size-3.5 shrink-0 text-primary" />
              {/if}
            </button>
          {/each}
        {/if}
      </div>

      <div
        class="shrink-0 border-t border-border/40 px-2.5 py-1.5 flex items-center justify-between"
      >
        <p class="font-mono text-ui-2xs text-muted-foreground">
          {databases.length} database{databases.length === 1 ? "" : "s"}
        </p>
        <button
          type="button"
          class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
          onclick={fetchDatabases}
          title="Refresh"
          ><RefreshCw class={cn("size-3", loading && "animate-spin")} /></button
        >
      </div>
    </div>
  {/if}
</div>
