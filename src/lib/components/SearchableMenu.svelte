<script>
  // Reusable searchable dropdown: a Popover anchored to a trigger, containing a
  // cmdk Command (search input + filtered, keyboard-navigable list). The input
  // auto-focuses on open; ArrowUp/Down move the highlight, Enter selects, Tab /
  // Shift+Tab also move the highlight, Esc closes. Used by the table toolbar's
  // Sort / Columns / Filter menus.
  import { Command, Popover } from "bits-ui";
  import { cn } from "$lib/utils.js";
  import SearchIcon from "@lucide/svelte/icons/search";

  let {
    open = $bindable(false),
    /** `group` shows a label above the first row of each run when nothing is typed.
     *  @type {Array<{ value: string, label?: string, keywords?: string[], disabled?: boolean, group?: string }>} */
    items = [],
    placeholder = "Search…",
    empty = "No results",
    closeOnSelect = true,
    align = "start",
    /** Popover side - use "top" for triggers anchored to the bottom (status bar). */
    side = "bottom",
    contentClass = "w-56",
    /** Show the search box only when there are more than this many items.
     *  Defaults to always showing it: a menu that grows a search field once it
     *  crosses some invisible count is a menu you can't build muscle memory for,
     *  so the field is always there whether the list is 3 items or 300. Set a
     *  positive number to opt a specific menu out. */
    searchThreshold = -1,
    /** @type {(item: any) => void} */
    onselect = () => {},
    /** Pointer came to rest on a row: a hint to start work the select will need.
     *  @type {((item: any) => void) | null} */
    onhover = null,
    /** snippet({ props }) - renders the trigger button */
    trigger,
    /** snippet(item) - renders one row's content */
    item,
    /** optional snippet() rendered above the list (e.g. a header bar) */
    header,
    /** optional snippet() rendered below the list (e.g. a footer action) */
    footer,
  } = $props();

  let search = $state("");
  const showSearch = $derived(searchThreshold < 0 || items.length > searchThreshold);
  /** @type {HTMLInputElement | null} */
  let inputEl = $state(null);
  /** @type {HTMLElement | null} */
  let listEl = $state(null);

  // Clear the query whenever the menu closes so it reopens fresh.
  $effect(() => { if (!open) search = ""; });

  function handleSelect(/** @type {any} */ it) {
    if (it?.disabled) return;
    // cmdk re-selects the first item on the items-changed re-render and scrolls
    // it into view, which jerks the list back to the top after a non-closing
    // toggle (e.g. show/hide columns). Capture the scroll position and restore it
    // so the user stays exactly where they were.
    const prevTop = listEl?.scrollTop ?? 0;
    onselect(it);
    if (closeOnSelect) {
      open = false;
    } else {
      // Refocus the search input after a non-closing select so cmdk's keyboard
      // selection cursor returns to the input and clears the item highlight.
      requestAnimationFrame(() => {
        inputEl?.focus();
        if (listEl && listEl.scrollTop !== prevTop) listEl.scrollTop = prevTop;
      });
    }
  }

  /**
   * Substring ranking instead of cmdk's default fuzzy score. The fuzzy score
   * matches scattered letters across every keyword, so "dyn" listed MariaDB,
   * DuckDB and PlanetScale (d…y…n somewhere in a description) above DynamoDB.
   * The first keyword is the row's label: a prefix of it ranks first, then a
   * word starting with it, then anywhere in it; other keywords (category,
   * description, id) match only as whole substrings, below any label match.
   * @param {string} value @param {string} query @param {string[]} [keywords]
   */
  function score(value, query, keywords) {
    const q = query.trim().toLowerCase();
    if (!q) return 1;
    const label = (keywords?.[0] ?? value).toLowerCase();
    if (label.startsWith(q)) return 1;
    if (label.split(/[\s/._-]+/).some((w) => w.startsWith(q))) return 0.9;
    if (label.includes(q)) return 0.8;
    const rest = [value, ...(keywords?.slice(1) ?? [])].join(" ").toLowerCase();
    return rest.includes(q) ? 0.5 : 0;
  }

  /** Rows for the current query, best match first (stable within a score). */
  const shown = $derived.by(() => {
    if (!search.trim()) return items;
    return items
      .map((it, i) => ({ it, i, s: score(it.value, search, [it.label ?? it.value, ...(it.keywords ?? [])]) }))
      .filter((r) => r.s > 0)
      .sort((a, b) => b.s - a.s || a.i - b.i)
      .map((r) => r.it);
  });

  // The highlighted row. With our own filtering, cmdk no longer moves it to the
  // top result as you type, so Enter picked whatever the previous query had
  // highlighted ("sql" then "dyn" + Enter chose SQL Server). Reset it per query.
  let highlighted = $state("");
  $effect(() => {
    search;
    highlighted = shown.find((it) => !it.disabled)?.value ?? "";
  });

  /** Esc closes; Tab / Shift+Tab map onto the Command's Arrow navigation. */
  function onInputKeydown(/** @type {KeyboardEvent} */ e) {
    if (e.key === "Escape") {
      // Close here and stop it bubbling to the app's global Escape handler
      // (which would otherwise close the row inspector / other UI).
      e.preventDefault();
      e.stopPropagation();
      open = false;
      return;
    }
    if (e.key === "Enter" && !e.isComposing) {
      // Typed and pressed Enter before the highlight caught up with the new
      // results: pick the top result rather than a row that is no longer shown.
      if (!shown.some((it) => it.value === highlighted && !it.disabled)) {
        const top = shown.find((it) => !it.disabled);
        e.preventDefault();
        e.stopImmediatePropagation();
        if (top) handleSelect(top);
        return true;
      }
      return;
    }
    if (e.key !== "Tab") return;
    e.preventDefault();
    const el = /** @type {HTMLElement} */ (e.currentTarget);
    el.dispatchEvent(new KeyboardEvent("keydown", { key: e.shiftKey ? "ArrowUp" : "ArrowDown", bubbles: true }));
  }
</script>

<!--
  cmdk always calls #selectFirstItem() on every re-render (sort/filter pass), so "id"
  gets data-selected any time the parent state changes, even if the mouse is over a
  different row. The rule below suppresses that background while the mouse is
  inside the list: the hovered item shows its own :hover highlight, and the stale
  cmdk auto-selection on other items is invisible. Keyboard navigation is unaffected
  because the mouse isn't hovering any item during pure keyboard use.
  It keys on the list's own :hover, not `:has([data-command-item]:hover)`: a
  :has() outside the subject makes WebKit re-check the whole document on every
  hover change anywhere in the app (see ui/alert/alert-title.svelte).
-->
<style>
  :global(.sm-item-list:hover [data-command-item][data-selected]:not(:hover)) {
    background-color: transparent !important;
    color: inherit !important;
  }
</style>

<Popover.Root bind:open>
  <Popover.Trigger>
    {#snippet child({ props })}
      {@render trigger(props)}
    {/snippet}
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content
      {align}
      {side}
      sideOffset={4}
      onOpenAutoFocus={(e) => { e.preventDefault(); inputEl?.focus(); }}
      class={cn(
        "z-[100] overflow-hidden rounded-[10px] border border-border/60 bg-popover text-popover-foreground elevate-2-rim outline-none",
        contentClass,
      )}
    >
      <Command.Root loop shouldFilter={false} bind:value={highlighted} class="flex max-h-72 flex-col">
        <!-- Input stays mounted (Command needs it for keyboard nav) but is only
             shown for longer lists; short menus hide it and are just scannable. -->
        <div data-slot="menu-search" class={cn("flex items-center gap-2 px-2.5 py-2", !showSearch && "sr-only")}>
          <SearchIcon class="size-3.5 shrink-0 text-muted-foreground" />
          <Command.Input bind:value={search} {placeholder}>
            {#snippet child({ props })}
              <input
                {...props}
                bind:value={search}
                bind:this={inputEl}
                onkeydown={(e) => { if (onInputKeydown(e) === true) return; /** @type {any} */ (props).onkeydown?.(e); }}
                {placeholder}
                class="no-focus-ring min-w-0 flex-1 bg-transparent text-ui-sm text-foreground outline-none placeholder:text-muted-foreground"
              />
            {/snippet}
          </Command.Input>
        </div>
        {@render header?.()}
        <Command.List bind:ref={listEl} class="sm-item-list min-h-0 flex-1 overflow-y-auto p-1">
          <Command.Empty class="px-2 py-5 text-center text-ui-xs text-muted-foreground">{empty}</Command.Empty>
          {#each shown as it, i (it.value)}
            <!-- Group labels (plain case, per the menu recipe) when the list is
                 browsed; a search ranks across groups, so it shows none. -->
            {#if !search.trim() && it.group && it.group !== shown[i - 1]?.group}
              <div class="px-2 pt-2 pb-1 text-ui-2xs font-medium text-muted-foreground first:pt-1" role="presentation">{it.group}</div>
            {/if}
            <!-- cmdk scores a row against `value` + `keywords`, never its rendered
                 content. `value` is the selection key, so whenever it is an id
                 (a uuid, an account id) the visible label is not searchable at all
                 and typing it filters everything out. Default the keywords to the
                 label so every menu is searchable by what it shows; callers can
                 still pass `keywords` to add aliases. -->
            <Command.Item
              value={it.value}
              keywords={[it.label ?? it.value, ...(it.keywords ?? [])]}
              disabled={it.disabled}
              onSelect={() => handleSelect(it)}
              onpointerenter={onhover ? () => onhover?.(it) : undefined}
              class={cn(
                "flex min-h-7 w-full min-w-0 cursor-default items-center gap-2 rounded-sm px-2 py-1 text-ui-xs outline-hidden select-none",
                "text-foreground/85 hover:bg-accent hover:text-foreground data-selected:bg-accent data-selected:text-foreground",
                "data-[disabled=true]:pointer-events-none data-[disabled=true]:opacity-40",
                "[&_svg]:pointer-events-none [&_svg]:shrink-0",
              )}
            >
              {@render item(it)}
            </Command.Item>
          {/each}
        </Command.List>
        {@render footer?.()}
      </Command.Root>
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>
