<script>
  import { tx } from '$lib/i18n.js'
  /**
   * The window's menu bar: File, Edit, View, Tools, Help.
   *
   * In-app rather than an OS menu, because the window is frameless on every
   * platform (see src-tauri/src/lib.rs) - a native menu would sit in a title bar
   * this app does not have, and on Linux/Windows would be drawn by the desktop in
   * its own theme, next to chrome drawn in ours.
   *
   * It is a real `Menubar`, not five dropdowns in a row: one tab stop for the
   * whole bar, arrow keys between menus, and hover switching menus once one is
   * open - all of which a row of independent dropdowns does not do, and all of
   * which people expect from anything shaped like this.
   *
   * Every item is an action the app already has. The bar is a second way to reach
   * them for people who look for a menu before they look for a shortcut; it owns
   * no behaviour of its own.
   *
   * @typedef {Object} Props
   * @property {boolean} [connected]
   * @property {Record<string, () => void>} actions
   */
  import { Menubar } from "bits-ui";
  import Icon from "./Icon.svelte";
  import Kbd from "./Kbd.svelte";
  import { COMMAND_BY_ID } from "$lib/shortcuts.js";

  let { connected = false, actions = /** @type {Record<string, () => void>} */ ({}) } = $props();
  let openMenu = $state('');
  let altHeld = $state(false);

  /** Map Alt+<letter> to the menu value, matching Windows/Linux native menu bars. */
  const MNEMONICS = /** @type {Record<string, string>} */ ({
    f: 'file',
    e: 'edit',
    v: 'view',
    t: 'tools',
    h: 'help',
  });

  /** @param {KeyboardEvent} e */
  function onWindowKeydown(e) {
    if (e.key === 'Alt' && !e.ctrlKey && !e.metaKey && !e.shiftKey) {
      altHeld = true;
      return;
    }
    if (e.altKey && !e.ctrlKey && !e.metaKey) {
      const target = /** @type {HTMLElement | null} */ (e.target);
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable || target.closest('.cm-editor'))) {
        return;
      }
      const id = MNEMONICS[e.key.toLowerCase()];
      if (id) {
        e.preventDefault();
        openMenu = openMenu === id ? '' : id;
      }
    }
  }
  /** @param {KeyboardEvent} e */
  function onWindowKeyup(e) {
    if (e.key === 'Alt') altHeld = false;
  }
  function onWindowBlur() {
    altHeld = false;
  }

  /** @param {string} name */
  function run(name) {
    actions[name]?.();
  }

  // h-6 on the control scale; normal weight so the five words read as a quiet
  // native menu row, not five buttons competing with the tabs.
  const triggerCls =
    "inline-flex h-6 max-h-[calc(26px*var(--app-scale))] shrink-0 items-center rounded-md px-2 text-ui-xs text-muted-foreground outline-none " +
    "hover:bg-foreground/[0.06] hover:text-foreground focus-visible:bg-foreground/[0.06] focus-visible:text-foreground " +
    "data-[state=open]:bg-foreground/[0.08] data-[state=open]:text-foreground";

  const contentCls =
    "data-closed:animate-out data-closed:fade-out-0 duration-75 " +
    "chrome-font border border-border/70 bg-popover text-popover-foreground min-w-60 max-w-(--menu-max-w) rounded-lg p-1 elevate-2-rim z-[60] outline-none";

  const itemCls =
    "focus:bg-accent focus:text-foreground h-7 gap-2.5 rounded-sm px-2 text-ui-xs relative flex min-w-0 cursor-default items-center outline-hidden select-none " +
    "data-disabled:pointer-events-none data-disabled:opacity-40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5";

  const sepCls = "-mx-1 my-1 h-px bg-border/50";
</script>

<svelte:window onkeydown={onWindowKeydown} onkeyup={onWindowKeyup} onblur={onWindowBlur} />

{#snippet menuLabel(/** @type {string} */ key, /** @type {string} */ fallback)}
  {@const text = $tx('menubar.' + key, fallback)}
  {#if altHeld && text.length > 0}<span class="underline underline-offset-2">{text[0]}</span>{text.slice(1)}{:else}{text}{/if}
{/snippet}

<!-- `combo` is a remappable command id (`tab.close`, printed as whatever the
     user bound it to) or a fixed chord in the grammar `createHotkey` parses (`Mod+Shift+T`),
     rendered by <Kbd> as one cap per key. The rows used to print the glyphs
     butted together as a single string - "Mod+Shift+T" gave `⌘⇧T`, three
     symbols with no shape at all, at 10px. -->
{#snippet item(/** @type {string} */ icon, /** @type {string} */ label, /** @type {string} */ action, /** @type {string} */ combo = "", /** @type {boolean} */ disabled = false)}
  <Menubar.Item class={itemCls} {disabled} onSelect={() => run(action)}>
    <Icon name={icon} class="size-3.5 shrink-0 text-muted-foreground" />
    <span data-slot="menu-label" class="min-w-0 truncate">{$tx('menubar.' + label, label)}</span>
    {#if COMMAND_BY_ID.has(combo)}<Kbd command={combo} class="ml-auto shrink-0 ps-6 text-muted-foreground" />
    {:else if combo}<Kbd {combo} class="ml-auto shrink-0 ps-6 text-muted-foreground" />{/if}
  </Menubar.Item>
{/snippet}


<Menubar.Root bind:value={openMenu} class="flex shrink-0 items-center gap-0.5" aria-label="Main menu">
  <!-- File -->
  <Menubar.Menu value="file">
    <Menubar.Trigger class={triggerCls}>{@render menuLabel('File', 'File')}</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("file-text", "New tab", "newTab", "tab.new")}
        {@render item("terminal", "New query editor", "newSql", "", !connected)}
        {@render item("copy", "New window", "newWindow", "window.new")}
        <Menubar.Separator class={sepCls} />
        {@render item("database", "Connect…", "openConnection")}
        {@render item("plug", "Disconnect", "disconnect", "connection.disconnect", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("settings", "Settings…", "openSettings", "settings.open")}
        <Menubar.Separator class={sepCls} />
        {@render item("x", "Close tab", "closeTab", "tab.close")}
        {@render item("x", "Close all tabs", "closeAllTabs", "tab.closeAll")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- Edit -->
  <Menubar.Menu value="edit">
    <Menubar.Trigger class={triggerCls}>{@render menuLabel('Edit', 'Edit')}</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("search", "Search rows", "search", "table.search", !connected)}
        {@render item("replace", "Find & replace…", "findReplace", "table.findReplaceAlt", !connected)}
        {@render item("list-filter", "Find in database…", "findInDatabase", "search.database", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("check", "Apply staged changes", "applyEdits", "table.apply", !connected)}
        {@render item("clipboard-copy", "Copy staged changes as SQL", "copyEditsSql", "table.copySql", !connected)}
        {@render item("undo-2", "Discard staged changes", "resetEdits", "table.discard", !connected)}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- View -->
  <Menubar.Menu value="view">
    <Menubar.Trigger class={triggerCls}>{@render menuLabel('View', 'View')}</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("layout-list", "Toggle sidebar", "toggleSidebar", "view.sidebar")}
        {@render item("bot", "Toggle chat", "toggleChat", "ai.sidebar")}
        <Menubar.Separator class={sepCls} />
        {@render item("plus", "Zoom in", "zoomIn", "Mod+Plus")}
        {@render item("minus", "Zoom out", "zoomOut", "Mod+Minus")}
        {@render item("rotate-ccw", "Reset zoom", "zoomReset", "Mod+0")}
        <Menubar.Separator class={sepCls} />
        {@render item("maximize-2", "Toggle full screen", "fullscreen", "view.fullscreen")}
        {@render item("command", "Command palette…", "commandPalette", "palette.open")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- Tools -->
  <Menubar.Menu value="tools">
    <Menubar.Trigger class={triggerCls}>{@render menuLabel('Tools', 'Tools')}</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("box", "Database objects", "objects", "", !connected)}
        {@render item("server", "Instance insights", "insights", "", !connected)}
        {@render item("git-branch", "Schema explorer", "schema", "", !connected)}
        {@render item("network", "Schema diagram", "erd", "", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("git-compare", "Data diff", "dataDiff", "", !connected)}
        {@render item("git-merge", "Migrations", "migrations", "", !connected)}
        {@render item("layout-dashboard", "Dashboard", "dashboard", "", !connected)}
        {@render item("history", "Activity log", "logs", "view.logs", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("database", "Local servers", "localServers", "")}
        {@render item("blocks", "Extensions", "extensions", "view.extensions")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- Help -->
  <Menubar.Menu value="help">
    <Menubar.Trigger class={triggerCls}>{@render menuLabel('Help', 'Help')}</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("command", "Keyboard shortcuts", "shortcuts", "help.shortcuts")}
        {@render item("external-link", "What's new", "changelog")}
        <Menubar.Separator class={sepCls} />
        {@render item("alert-triangle", "Report an issue…", "reportIssue")}
        {@render item("refresh-cw", "Check for updates", "checkUpdates")}
        {@render item("info", "About Stroke", "about")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>
</Menubar.Root>
