<script>
  // Connections sidebar panel - VSCode-style switchable panel that lists saved
  // connections, lets the user switch between them, add new ones, and remove
  // them. Connection lifecycle actions (switch/add/remove/disconnect) are
  // delegated to the shell. Group assignment persists locally via the store and
  // also emits `onsetgroup` so the shell can refresh its own copy of the list.
  import Icon from "./Icon.svelte";
  import { cn } from "$lib/utils.js";
  import { Popover, PopoverTrigger, PopoverContent } from "$lib/components/ui/popover/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { setConnectionGroup } from "$lib/stores/connections.js";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection[]} */
    connections = [],
    /** id of the currently live connection (highlighted row). */
    activeId = "",
    /** @type {(c: import('$lib/stores/connections.js').SavedConnection) => void} */
    onswitch = (c) => {},
    onadd = () => {},
    /** @type {(id: string) => void} */
    onremove = (id) => {},
    ondisconnect = () => {},
    /** Fired after a connection's group changes so the shell can refresh its list. @type {(id: string, group: string | null) => void} */
    onsetgroup = (id, group) => {},
  } = $props();

  // Removing deletes the connection's history and saved work too, so it asks first.
  /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
  let pendingRemove = $state(null);
  let confirmRemoveOpen = $state(false);

  // Optimistic overlay of pending group edits (id -> group|null). Keeps the list
  // updating instantly even before the parent refreshes the `connections` prop.
  let overrides = $state(/** @type {Record<string, string | null>} */ ({}));
  // Which row's group popover is open, and the draft name for a brand-new group.
  let groupMenuFor = $state(/** @type {string | null} */ (null));
  let draftGroup = $state("");
  // Collapsed group sections, keyed by group name.
  let collapsed = $state(/** @type {Record<string, boolean>} */ ({}));

  /**
   * Effective (overlay-aware) group for a connection; empty/whitespace -> null.
   * @param {import('$lib/stores/connections.js').SavedConnection} c
   * @returns {string | null}
   */
  function effGroup(c) {
    const raw = c.id in overrides ? overrides[c.id] : (c.group ?? null);
    return raw && String(raw).trim() ? String(raw).trim() : null;
  }

  /** Sorted, de-duplicated list of group names currently in use. */
  const groupNames = $derived.by(() => {
    const set = new Set();
    for (const c of connections) {
      const g = effGroup(c);
      if (g) set.add(g);
    }
    return [...set].sort((a, b) => a.localeCompare(b));
  });

  /**
   * Connections split into sorted named groups + an Ungrouped bucket. When no
   * connection has a group the panel renders a flat list (no section headers),
   * so the feature is invisible until it is used.
   */
  const grouped = $derived.by(() => {
    /** @type {Map<string, import('$lib/stores/connections.js').SavedConnection[]>} */
    const named = new Map();
    /** @type {import('$lib/stores/connections.js').SavedConnection[]} */
    const ungrouped = [];
    for (const c of connections) {
      const g = effGroup(c);
      if (g) {
        const bucket = named.get(g) ?? named.set(g, []).get(g);
        bucket.push(c);
      } else {
        ungrouped.push(c);
      }
    }
    const groups = [...named.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([name, items]) => ({ name, items }));
    return { groups, ungrouped };
  });

  /** @param {string} name */
  function toggleCollapsed(name) {
    collapsed = { ...collapsed, [name]: !collapsed[name] };
  }

  /**
   * Assign (or clear, with null/empty) a connection's group: persist locally,
   * update the optimistic overlay, notify the shell, and close the popover.
   * @param {import('$lib/stores/connections.js').SavedConnection} c
   * @param {string | null} group
   */
  function applyGroup(c, group) {
    const g = group && String(group).trim() ? String(group).trim() : null;
    overrides = { ...overrides, [c.id]: g };
    setConnectionGroup(c.id, g);
    onsetgroup(c.id, g);
    groupMenuFor = null;
    draftGroup = "";
  }

  /**
   * Per-engine leading icon. All engines map to `database` today - kept as a
   * lookup so a per-engine glyph can be swapped in later without touching the row.
   * @param {string | undefined} type
   */
  function engineIcon(type) {
    switch (type) {
      default:
        return "database";
    }
  }

  /** @param {import('$lib/stores/connections.js').SavedConnection} c */
  function subtitle(c) {
    return c.host ?? c.filePath ?? c.database ?? c.type ?? "";
  }
</script>

{#snippet connRow(/** @type {import('$lib/stores/connections.js').SavedConnection} */ c)}
  {@const active = c.id === activeId}
  <div
    class={cn(
      "group/conn flex min-w-0 items-center rounded-md transition-colors",
      active ? "bg-accent/40" : "hover:bg-accent/20",
    )}
  >
    <button
      type="button"
      title={active ? `${c.name} (connected)` : `Switch to ${c.name}`}
      aria-current={active ? "true" : undefined}
      onclick={() => (active ? undefined : onswitch(c))}
      class="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5 text-left"
    >
      <span class="relative flex size-4 shrink-0 items-center justify-center text-muted-foreground">
        <Icon name={engineIcon(c.type)} class="size-4" />
        {#if active}
          <span class="absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full bg-primary ring-2 ring-sidebar"></span>
        {/if}
      </span>
      <span class="flex min-w-0 flex-1 flex-col">
        <span class="truncate text-ui-sm font-medium text-sidebar-foreground">{c.name}</span>
        <span class="truncate text-ui-2xs text-muted-foreground">{subtitle(c)}</span>
      </span>
    </button>
    <!-- Trailing state / actions -->
    <div class="flex shrink-0 items-center gap-0.5 pr-1.5">
      <!-- Group assignment -->
      <Popover
        open={groupMenuFor === c.id}
        onOpenChange={(o) => {
          if (o) {
            groupMenuFor = c.id;
            draftGroup = "";
          } else if (groupMenuFor === c.id) {
            groupMenuFor = null;
          }
        }}
      >
        <PopoverTrigger
          title="Set group"
          aria-label="Set group"
          class={cn(
            "size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
            groupMenuFor === c.id ? "flex bg-accent text-foreground" : "hidden group-hover/conn:flex",
          )}
        >
          <Icon name="folder-open" class="size-3.5" />
        </PopoverTrigger>
        <PopoverContent align="end" sideOffset={4} class="min-w-56 p-1.5 text-ui-xs">
          <div class="px-1.5 py-1 text-ui-3xs font-medium uppercase tracking-wider text-muted-foreground">
            Group
          </div>
          {#each groupNames as g (g)}
            <button
              type="button"
              onclick={() => applyGroup(c, g)}
              class="flex w-full min-w-0 items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-ui-xs transition-colors hover:bg-accent hover:text-foreground"
            >
              <Icon name="folder-open" class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="min-w-0 flex-1 truncate">{g}</span>
              {#if effGroup(c) === g}
                <Icon name="check" class="size-3.5 shrink-0 text-primary" />
              {/if}
            </button>
          {/each}
          {#if effGroup(c)}
            <button
              type="button"
              onclick={() => applyGroup(c, null)}
              class="flex w-full min-w-0 items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            >
              <Icon name="x" class="size-3.5 shrink-0" />
              <span class="min-w-0 flex-1 truncate">Remove from group</span>
            </button>
          {/if}
          <form
            class="mt-1 border-t border-border/50 pt-1.5"
            onsubmit={(e) => {
              e.preventDefault();
              if (draftGroup.trim()) applyGroup(c, draftGroup);
            }}
          >
            <Input
              bind:value={draftGroup}
              placeholder="New group…"
              aria-label="New group name"
              class="h-7 rounded-md border px-2 text-ui-xs"
            />
          </form>
        </PopoverContent>
      </Popover>
      {#if active}
        <span class="text-ui-2xs font-medium text-primary group-hover/conn:hidden">connected</span>
        <button
          type="button"
          title="Disconnect"
          aria-label="Disconnect"
          onclick={(e) => {
            e.stopPropagation();
            ondisconnect();
          }}
          class="hidden size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/conn:flex"
        >
          <Icon name="unplug" class="size-3.5" />
        </button>
      {:else}
        <button
          type="button"
          title="Remove connection"
          aria-label="Remove connection"
          onclick={(e) => {
            e.stopPropagation();
            pendingRemove = c;
            confirmRemoveOpen = true;
          }}
          class="hidden size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/conn:flex"
        >
          <Icon name="trash-2" class="size-3.5" />
        </button>
      {/if}
    </div>
  </div>
{/snippet}

{#snippet groupHeader(/** @type {string} */ name, /** @type {number} */ count)}
  <button
    type="button"
    onclick={() => toggleCollapsed(name)}
    class="mt-1 flex w-full min-w-0 items-center gap-1 rounded-md px-1.5 py-1 text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
  >
    <Icon name={collapsed[name] ? "chevron-right" : "chevron-down"} class="size-3.5 shrink-0" />
    <Icon name="folder-open" class="size-3.5 shrink-0" />
    <span class="min-w-0 flex-1 truncate text-left font-medium">{name}</span>
    <span class="shrink-0 tabular-nums text-muted-foreground">{count}</span>
  </button>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  <!-- Sticky header -->
  <div class="flex h-9 shrink-0 items-center gap-1 px-2.5">
    <span class="min-w-0 flex-1 truncate text-ui-3xs font-medium uppercase tracking-wider text-muted-foreground">
      Connections
    </span>
    <button
      type="button"
      title="Add connection"
      aria-label="Add connection"
      onclick={() => onadd()}
      class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
    >
      <Icon name="plus" class="size-4" />
    </button>
  </div>

  <!-- List -->
  <div class="app-scroll min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain px-1.5 pb-2">
    {#if connections.length === 0}
      <div class="flex flex-col items-center gap-2 px-4 py-10 text-center">
        <Icon name="database" class="size-5 text-muted-foreground" />
        <p class="text-ui-xs text-muted-foreground">No saved connections</p>
        <button
          type="button"
          onclick={() => onadd()}
          class="mt-1 rounded-md border border-border bg-background/40 px-2.5 py-1 text-ui-xs font-medium transition-colors hover:bg-accent/40 hover:text-foreground"
        >
          Add connection
        </button>
      </div>
    {:else if grouped.groups.length === 0}
      <!-- No groups in use: flat list, visually unchanged from before. -->
      {#each connections as c (c.id)}
        {@render connRow(c)}
      {/each}
    {:else}
      {#each grouped.groups as section (section.name)}
        {@render groupHeader(section.name, section.items.length)}
        {#if !collapsed[section.name]}
          {#each section.items as c (c.id)}
            {@render connRow(c)}
          {/each}
        {/if}
      {/each}
      {#if grouped.ungrouped.length > 0}
        {@render groupHeader("Ungrouped", grouped.ungrouped.length)}
        {#if !collapsed["Ungrouped"]}
          {#each grouped.ungrouped as c (c.id)}
            {@render connRow(c)}
          {/each}
        {/if}
      {/if}
    {/if}
  </div>
</div>

<ConfirmDialog
  bind:open={confirmRemoveOpen}
  icon="trash-2"
  title="Delete connection?"
  description={`"${pendingRemove?.name || pendingRemove?.database || pendingRemove?.host || pendingRemove?.filePath || 'this connection'}" will be removed from your saved connections.`}
  note="Its query history, saved queries, charts and AI chats are deleted with it. This can't be undone."
  confirmLabel="Delete"
  confirmIcon="trash-2"
  variant="destructive"
  onconfirm={() => {
    if (pendingRemove) onremove(pendingRemove.id);
    pendingRemove = null;
  }}
  oncancel={() => (pendingRemove = null)}
/>
