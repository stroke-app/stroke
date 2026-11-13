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
  import { setConnectionGroup, setConnectionMeta } from "$lib/stores/connections.js";
  import { workspaces, activeWorkspaceId, inWorkspace, workspaceOf, colorOf } from "$lib/stores/workspaces.js";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ColorSwatches from "./ColorSwatches.svelte";

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
    /** Fired after a connection's colour or workspace changes, with the saved list. @type {(list: import('$lib/stores/connections.js').SavedConnection[]) => void} */
    onmetachange = (list) => {},
  } = $props();

  // All saved connections feed the switcher's counts; the list shows the
  // active workspace's.
  const allConnections = $derived(connections);
  // Pinned connections first; the sort is stable, so the rest keep their order.
  const shown = $derived(
    inWorkspace(allConnections, $activeWorkspaceId, $workspaces)
      .slice()
      .sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned)),
  );

  /** @param {import('$lib/stores/connections.js').SavedConnection} c @param {{ workspaceId?: string | null, color?: string | null, pinned?: boolean }} patch */
  function applyMeta(c, patch) {
    onmetachange(setConnectionMeta(c.id, patch));
  }

  // Removing deletes the connection's history and saved work too, so it asks first.
  /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
  let pendingRemove = $state(null);
  let confirmRemoveOpen = $state(false);

  // Optimistic overlay of pending group edits (id -> group|null). Keeps the list
  // updating instantly even before the parent refreshes the `connections` prop.
  let overrides = $state(/** @type {Record<string, string | null>} */ ({}));
  // Which row's group popover is open, and the draft name for a brand-new group.
  let groupMenuFor = $state(/** @type {string | null} */ (null));
  /**
   * The custom-colour swatch opens the OS colour panel, which takes focus from
   * the window: to the popover that is a focus-outside, and it closed mid-pick.
   * Stay open while the window has no focus or the colour input is active.
   * @param {Event} e
   */
  function keepOpenWhilePicking(e) {
    const active = document.activeElement;
    if (!document.hasFocus() || (active instanceof HTMLInputElement && active.type === "color")) {
      e.preventDefault();
    }
  }
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
    for (const c of shown) {
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
    for (const c of shown) {
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
      active ? "bg-sidebar-accent" : "hover:bg-sidebar-accent/60",
    )}
  >
    <button
      type="button"
      title={`${active ? `${c.name} (connected)` : `Switch to ${c.name}`}${subtitle(c) ? `\n${subtitle(c)}` : ''}`}
      aria-current={active ? "true" : undefined}
      onclick={() => (active ? undefined : onswitch(c))}
      class="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5 text-left"
    >
      <span style={colorOf(c.color)?.style} class={cn("relative flex size-4 shrink-0 items-center justify-center", colorOf(c.color)?.text ?? "text-muted-foreground")}>
        <Icon name={engineIcon(c.type)} class="size-4 shrink-0" />
        {#if active}
          <span class="absolute -bottom-0.5 -right-0.5 size-1.5 rounded-full bg-primary ring-2 ring-sidebar"></span>
        {/if}
      </span>
      <span class="flex min-w-0 flex-1 flex-col">
        <span class="flex min-w-0 items-center gap-1.5">
          {#if colorOf(c.color)}<span style={colorOf(c.color)?.style} class={cn("size-1.5 shrink-0 rounded-full", colorOf(c.color)?.dot)} aria-label={`${colorOf(c.color)?.label} label`}></span>{/if}
          <span class={cn("truncate text-ui-sm", active ? "font-medium text-foreground" : "text-sidebar-foreground")}>{c.name}</span>
          {#if c.pinned}<Icon name="pin" class="size-3 shrink-0 text-muted-foreground" aria-label="Pinned" />{/if}
        </span>
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
          title="Colour, workspace and group"
          aria-label="Colour, workspace and group"
          class={cn(
            "size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
            groupMenuFor === c.id ? "flex bg-accent text-foreground" : "hidden group-hover/conn:flex",
          )}
        >
          <Icon name="more-horizontal" class="size-3.5 shrink-0" />
        </PopoverTrigger>
        <PopoverContent
          align="end"
          sideOffset={4}
          class="min-w-60 p-1.5 text-ui-xs"
          onFocusOutside={keepOpenWhilePicking}
          onInteractOutside={keepOpenWhilePicking}
        >
          <button
            type="button"
            onclick={() => { applyMeta(c, { pinned: !c.pinned }); groupMenuFor = null; }}
            class="mb-1 flex w-full min-w-0 items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-ui-xs hover:bg-accent hover:text-foreground"
          >
            <Icon name={c.pinned ? "pin-off" : "pin"} class="size-3.5 shrink-0 text-muted-foreground" />
            <span class="min-w-0 flex-1 truncate">{c.pinned ? "Unpin connection" : "Pin connection"}</span>
          </button>
          <div class="px-1.5 py-1 text-ui-3xs font-medium text-muted-foreground">Colour</div>
          <div class="px-1.5 pb-2 pt-0.5">
            <ColorSwatches value={c.color ?? null} allowNone onchange={(col) => applyMeta(c, { color: col })} />
          </div>
          {#if $workspaces.length > 1}
            <div class="border-t border-border/50 px-1.5 pt-2 pb-1 text-ui-3xs font-medium text-muted-foreground">Workspace</div>
            {#each $workspaces as w (w.id)}
              <button
                type="button"
                onclick={() => { applyMeta(c, { workspaceId: w.id === "default" ? null : w.id }); groupMenuFor = null; }}
                class="flex w-full min-w-0 items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-ui-xs hover:bg-accent hover:text-foreground"
              >
                <span style={colorOf(w.color)?.style} class={cn("size-2 shrink-0 rounded-full", colorOf(w.color)?.dot)}></span>
                <span class="min-w-0 flex-1 truncate">{w.name}</span>
                {#if workspaceOf(c, $workspaces) === w.id}<Icon name="check" class="size-3.5 shrink-0 text-foreground" />{/if}
              </button>
            {/each}
          {/if}
          <div class="border-t border-border/50 px-1.5 pt-2 pb-1 text-ui-3xs font-medium text-muted-foreground">Group</div>
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
        <span class="text-ui-2xs text-muted-foreground group-hover/conn:hidden">connected</span>
        <button
          type="button"
          title="Disconnect"
          aria-label="Disconnect"
          onclick={(e) => {
            e.stopPropagation();
            ondisconnect();
          }}
          class="hidden size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/conn:flex"
        >
          <Icon name="unplug" class="size-3.5 shrink-0" />
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
          class="hidden size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground group-hover/conn:flex"
        >
          <Icon name="trash-2" class="size-3.5 shrink-0" />
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
  <div class="flex h-9 shrink-0 items-center gap-1 px-2">
    <span class="min-w-0 flex-1 truncate px-1.5 text-ui-xs font-medium text-muted-foreground">Connections</span>
    {#if shown.length}<span class="shrink-0 px-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">{shown.length}</span>{/if}
    <button
      type="button"
      title="Add connection"
      aria-label="Add connection"
      onclick={() => onadd()}
      class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >
      <Icon name="plus" class="size-4 shrink-0" />
    </button>
  </div>

  <!-- List -->
  <div class="app-scroll min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain px-1.5 pb-2">
    {#if shown.length === 0}
      <div class="flex flex-col items-center gap-2 px-4 py-10 text-center">
        <p class="text-ui-sm text-foreground">{connections.length ? "No connections in this workspace" : "No saved connections"}</p>
        <button
          type="button"
          onclick={() => onadd()}
          class="h-7 rounded-md px-2.5 text-ui-xs text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground"
        >
          Add connection
        </button>
      </div>
    {:else if grouped.groups.length === 0}
      <!-- No groups in use: flat list, visually unchanged from before. -->
      {#each shown as c (c.id)}
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
