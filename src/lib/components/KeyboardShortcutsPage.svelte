<script>
  /**
   * Every shortcut, by group, searchable and remappable in place. Opens as a
   * tab (Keyboard shortcuts, ⌘/); KeyboardShortcutsDialog shows the same page
   * as an overlay when there is no tab bar to put it in.
   */
  import { tick } from "svelte";
  import { cn } from "$lib/utils.js";
  import Keyboard from "@lucide/svelte/icons/keyboard";
  import Search from "@lucide/svelte/icons/search";
  import Navigation from "@lucide/svelte/icons/navigation";
  import Monitor from "@lucide/svelte/icons/monitor";
  import Terminal from "@lucide/svelte/icons/terminal";
  import Code2 from "@lucide/svelte/icons/code-2";
  import Table2 from "@lucide/svelte/icons/table-2";
  import Bot from "@lucide/svelte/icons/bot";
  import Palette from "@lucide/svelte/icons/palette";
  import Settings from "@lucide/svelte/icons/settings";
  import Network from "@lucide/svelte/icons/network";
  import X from "@lucide/svelte/icons/x";

  import { SHORTCUT_GROUPS, keycaps, comboText, IS_MAC, rowCommandIds } from "$lib/shortcuts.js";
  import ShortcutRecorder from "./ShortcutRecorder.svelte";
  import Kbd from "./Kbd.svelte";
  import { keybindings, effectiveShortcutGroups } from "$lib/stores/keybindings.js";

  let {
    /** Shown: clears the search and focuses it each time this turns true. */
    active = true,
    /** Close control in the header; none when the page lives in a tab (the tab has its own). @type {(() => void) | null} */
    onclose = null,
  } = $props();

  // Platform flag comes from the index too, so this dialog and the bindings
  // agree on what machine they are running on.
  const isMac = IS_MAC;
  const mod = isMac ? "⌘" : "Ctrl";

  // The shortcut list lives in $lib/shortcuts.js. It used to be written out here
  // as per-platform keycap arrays, which meant this dialog could - and did -
  // describe a binding the app no longer had. Groups now carry an icon NAME so
  // that module can stay plain data; the components stay here.
  /** @type {Record<string, any>} */
  const GROUP_ICONS = {
    navigation: Navigation,
    monitor: Monitor,
    terminal: Terminal,
    "code-2": Code2,
    "table-2": Table2,
    bot: Bot,
    palette: Palette,
    settings: Settings,
    network: Network,
  };

  /**
   * @typedef {{ combo: string; desc: string }} Shortcut
   * @typedef {{ label: string; icon: string; shortcuts: Shortcut[] }} Group
   */

  /** What the app answers to now: remapped rows show the user's keys. */
  /** @type {Group[]} */
  const groups = $derived(effectiveShortcutGroups($keybindings));

  let query = $state("");
  let selectedGroup = $state(SHORTCUT_GROUPS[0].label);
  /** @type {HTMLInputElement | null} */
  let searchEl = $state(null);

  // The component stays mounted across opens, so clear the stale query each
  // time - and focus the search box so the advertised ⌘F/Esc keys work without
  // first clicking inside it. selectedGroup deliberately persists.
  $effect(() => {
    if (active) {
      query = "";
      tick().then(() => searchEl?.focus());
    }
  });

  const isSearching = $derived(query.trim().length > 0);

  const filteredGroups = $derived.by(() => {
    const q = query.toLowerCase().trim();
    if (!q) return groups;
    return groups
      .map((g) => ({
        ...g,
        shortcuts: g.shortcuts.filter(
          (s) =>
            s.desc.toLowerCase().includes(q) ||
            comboText(s.combo).includes(q),
        ),
      }))
      .filter((g) => g.shortcuts.length > 0);
  });

  const displayGroups = $derived(
    isSearching
      ? filteredGroups
      : groups.filter((g) => g.label === selectedGroup),
  );

  /** @param {KeyboardEvent} e */
  function handleGlobalKey(e) {
    if (e.key === "Escape" && onclose) {
      e.preventDefault();
      onclose();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "f") {
      e.preventDefault();
      searchEl?.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex min-h-0 flex-1 flex-col bg-background" onkeydown={handleGlobalKey}>
  <!-- Header -->
  <div class="flex h-14 shrink-0 items-center gap-4 border-b border-border/30 px-5">
    <h1 class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">Keyboard shortcuts</h1>

    <!-- Search -->
    <div class="relative w-64 shrink-0">
      <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        bind:this={searchEl}
        type="text"
        placeholder="Search shortcuts…"
        bind:value={query}
        class="no-field-frame h-8 w-full rounded-full border-0 bg-sidebar-accent/50 pl-8 pr-7 text-ui-xs text-foreground outline-none placeholder:text-muted-foreground focus:bg-sidebar-accent"
      />
      {#if query}
        <button
          type="button"
          class="absolute top-1/2 right-2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
          onclick={() => (query = "")}
        >
          <X class="size-3 shrink-0" />
        </button>
      {/if}
    </div>

    {#if onclose}
      <button
        type="button"
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={onclose}
        aria-label="Close"
        title="Close (Esc)"
      >
        <X class="size-3.5 shrink-0" />
      </button>
    {/if}
  </div>

  <!-- Body -->
  <div class="flex min-h-0 flex-1 overflow-hidden">

    <!-- Sidebar -->
    {#if !isSearching}
      <nav class="flex w-52 shrink-0 flex-col gap-px overflow-y-auto border-r border-border/30 bg-sidebar px-2 py-3">
        {#each groups as group (group.label)}
          {@const Icon = GROUP_ICONS[group.icon]}
          {@const active = selectedGroup === group.label}
          <button
            type="button"
            onclick={() => (selectedGroup = group.label)}
            class={cn(
              "group flex h-8 w-full items-center gap-2.5 rounded-lg px-2.5 text-left text-ui-xs",
              active
                ? "bg-sidebar-accent text-foreground"
                : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground",
            )}
          >
            <Icon
              class={cn(
                "size-3.5 shrink-0 transition-colors",
                active ? "text-foreground/80" : "text-muted-foreground group-hover:text-muted-foreground",
              )}
            />
            {group.label}
            <span class={cn(
              "ml-auto text-ui-2xs tabular-nums text-muted-foreground/70",
              active ? "text-muted-foreground" : "text-muted-foreground group-hover:text-muted-foreground",
            )}>
              {group.shortcuts.length}
            </span>
          </button>
        {/each}
      </nav>
    {/if}

    <!-- Content -->
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if displayGroups.length === 0}
        <div class="flex flex-col items-center gap-3 py-20 text-center">
          <div class="flex size-10 items-center justify-center rounded-full border border-border/30 bg-muted/30">
            <Search class="size-4 shrink-0 text-muted-foreground" />
          </div>
          <p class="text-ui-sm text-muted-foreground">No shortcuts match <span class="text-foreground/60">"{query}"</span></p>
        </div>
      {:else}
        <div class="mx-auto flex w-full max-w-3xl flex-col gap-6 px-6 py-6">
          {#each displayGroups as group (group.label)}
            <section>
              <h2 class="mb-1.5 px-3 text-ui-xs font-medium text-foreground/80">{group.label}</h2>
              <ul class="flex flex-col gap-px">
                {#each group.shortcuts as shortcut, i (group.label + ':' + i)}
                  {@const caps = keycaps(shortcut.combo)}
                  {@const ids = rowCommandIds(/** @type {any} */ (shortcut))}
                  <li
                    class={cn(
                      "group/row flex min-h-9 items-center justify-between gap-6 rounded-lg px-3 py-1 transition-colors hover:bg-accent/50",
                    )}
                  >
                    <span class="min-w-0 text-ui-xs text-foreground/85 group-hover/row:text-foreground">
                      {shortcut.desc}
                    </span>

                    <!-- A single command is remappable right here; ranges
                         (⌘1-9) and built-in keys only show their caps. No `+`
                         between the caps: three chips already read as one chord. -->
                    {#if ids.length === 1}
                      <ShortcutRecorder id={ids[0]} label={shortcut.desc} />
                    {:else}
                      <!-- pr-9: the pencil slot editable rows reserve, so every row's caps share one right edge. -->
                      <span class="flex shrink-0 items-center pr-9"><Kbd keys={caps} size="md" class="gap-1" /></span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
        </div>
      {/if}
    </div>

  </div>
</div>
