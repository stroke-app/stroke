<script>
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

  let { connected = false, actions = /** @type {Record<string, () => void>} */ ({}) } = $props();
  /** @param {string} name */
  function run(name) {
    actions[name]?.();
  }

  // h-6 on the control scale, not an off-scale 22px; normal weight so the five
  // words read as a quiet menu row, not five buttons competing with the tabs.
  const triggerCls =
    "inline-flex h-6 shrink-0 items-center rounded-md px-2.5 text-ui-2xs text-muted-foreground outline-none transition-colors " +
    "hover:bg-foreground/[0.06] hover:text-foreground focus-visible:bg-foreground/[0.06] focus-visible:text-foreground " +
    "data-[state=open]:bg-foreground/[0.08] data-[state=open]:text-foreground";

  const contentCls =
    "data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 duration-75 " +
    "border border-border/60 bg-popover text-popover-foreground min-w-60 max-w-(--menu-max-w) rounded-[10px] p-1 elevate-2-rim z-[60] outline-none";

  // A fixed h-7 row (the compact control height) instead of py-1: padding-based
  // rows grew with the font and zoom, and every menu came out a different
  // rhythm. rounded-md inside the 10px menu with 4px padding keeps the corners
  // concentric.
  const itemCls =
    "focus:bg-accent focus:text-foreground h-7 gap-2.5 rounded-md px-2 text-ui-xs relative flex min-w-0 cursor-default items-center outline-hidden select-none " +
    "data-disabled:pointer-events-none data-disabled:opacity-40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5";

  const sepCls = "-mx-1 my-1 h-px bg-border/50";
</script>

<!-- `combo` is a chord in the grammar `createHotkey` parses (`Mod+Shift+T`),
     rendered by <Kbd> as one cap per key. The rows used to print the glyphs
     butted together as a single string - "Mod+Shift+T" gave `⌘⇧T`, three
     symbols with no shape at all, at 10px. -->
{#snippet item(/** @type {string} */ icon, /** @type {string} */ label, /** @type {string} */ action, /** @type {string} */ combo = "", /** @type {boolean} */ disabled = false)}
  <Menubar.Item class={itemCls} {disabled} onSelect={() => run(action)}>
    <Icon name={icon} class="size-3.5 text-muted-foreground" />
    <span data-slot="menu-label" class="min-w-0 truncate">{label}</span>
    {#if combo}<Kbd {combo} class="ml-auto shrink-0 ps-6 text-muted-foreground/80" />{/if}
  </Menubar.Item>
{/snippet}


<Menubar.Root class="flex shrink-0 items-center gap-0.5" aria-label="Main menu">
  <!-- File -->
  <Menubar.Menu>
    <Menubar.Trigger class={triggerCls}>File</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("file-text", "New tab", "newTab", "Mod+Shift+T")}
        {@render item("terminal", "New query editor", "newSql", "", !connected)}
        {@render item("copy", "New window", "newWindow", "Mod+Shift+N")}
        <Menubar.Separator class={sepCls} />
        {@render item("database", "Connect…", "openConnection")}
        {@render item("plug", "Disconnect", "disconnect", "Mod+Shift+D", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("settings", "Settings…", "openSettings", "Mod+,")}
        {@render item("x", "Close tab", "closeTab", "Mod+W")}
        {@render item("trash-2", "Close all tabs", "closeAllTabs", "Mod+Shift+W")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- Edit -->
  <Menubar.Menu>
    <Menubar.Trigger class={triggerCls}>Edit</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("search", "Search rows", "search", "Mod+F", !connected)}
        {@render item("replace", "Find & replace…", "findReplace", "Mod+Alt+F", !connected)}
        {@render item("list-filter", "Find in database…", "findInDatabase", "", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("check", "Apply staged changes", "applyEdits", "Mod+S", !connected)}
        {@render item("clipboard-copy", "Copy staged changes as SQL", "copyEditsSql", "Mod+Alt+S", !connected)}
        {@render item("undo-2", "Discard staged changes", "resetEdits", "", !connected)}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- View -->
  <Menubar.Menu>
    <Menubar.Trigger class={triggerCls}>View</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("layout-list", "Toggle sidebar", "toggleSidebar", "Mod+B")}
        {@render item("bot", "Toggle chat", "toggleChat", "Mod+I")}
        <Menubar.Separator class={sepCls} />
        {@render item("plus", "Zoom in", "zoomIn", "Mod+Plus")}
        {@render item("minus", "Zoom out", "zoomOut", "Mod+Minus")}
        {@render item("rotate-ccw", "Reset zoom", "zoomReset", "Mod+0")}
        <Menubar.Separator class={sepCls} />
        {@render item("maximize-2", "Toggle full screen", "fullscreen", "F11")}
        {@render item("command", "Command palette…", "commandPalette", "Mod+K")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- Tools -->
  <Menubar.Menu>
    <Menubar.Trigger class={triggerCls}>Tools</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("box", "Database objects", "objects", "", !connected)}
        {@render item("server", "Instance insights", "insights", "", !connected)}
        {@render item("git-branch", "Schema explorer", "schema", "", !connected)}
        {@render item("network", "ER diagram", "erd", "", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("git-compare", "Data diff", "dataDiff", "", !connected)}
        {@render item("layout-dashboard", "Dashboard", "dashboard", "", !connected)}
        {@render item("history", "Activity log", "logs", "Mod+Shift+L", !connected)}
        <Menubar.Separator class={sepCls} />
        {@render item("blocks", "Extensions", "extensions", "Mod+Shift+X")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>


  <!-- Help -->
  <Menubar.Menu>
    <Menubar.Trigger class={triggerCls}>Help</Menubar.Trigger>
    <Menubar.Portal>
      <Menubar.Content class={contentCls} align="start" sideOffset={6}>
        {@render item("command", "Keyboard shortcuts", "shortcuts", "Mod+/")}
        {@render item("external-link", "What's new", "changelog")}
        <Menubar.Separator class={sepCls} />
        {@render item("alert-triangle", "Report an issue…", "reportIssue")}
        {@render item("refresh-cw", "Check for updates", "checkUpdates")}
        {@render item("info", "About Stroke", "about")}
      </Menubar.Content>
    </Menubar.Portal>
  </Menubar.Menu>
</Menubar.Root>
