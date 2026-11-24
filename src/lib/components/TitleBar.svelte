<script>
  import DbIcon from './DbIcon.svelte'
  import { onMount } from 'svelte'
  import MessageSquare from '@lucide/svelte/icons/message-square'
  import { cn } from '$lib/utils.js'
  import { detectOs } from '$lib/platform.js'
  import { isTrialActive, licenseStatus } from '$lib/stores/license.js'
  import Logo from './Logo.svelte'
  import AppMenuBar from './AppMenuBar.svelte'
  import { appMenuBar } from '$lib/stores/settings.js'
  import WindowControls from './WindowControls.svelte'
  import TitleBarMenu from './TitleBarMenu.svelte'
  import WorkspaceSwitcher from './WorkspaceSwitcher.svelte'
  import PanelLeft from '@lucide/svelte/icons/panel-left'
  import { tabDisplayTitle } from '$lib/studio-tabs.js'
  import { bindNativeMenu, syncNativeMenuState, watchFullscreen } from '$lib/native-menu.js'

  const isMac = typeof navigator !== 'undefined' && detectOs() === 'macos'

  // Window is frameless everywhere (see src-tauri/src/lib.rs), so this bar is
  // the drag region and double-click-to-maximize target.
  let isTauri = $state(false)
  onMount(() => {
    isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  })

  // The whole bar is the drag region, tagged `deep`: a bare
  // `data-tauri-drag-region` only drags when the click lands on that exact
  // element, so every child without the tag (the room beside the traffic
  // lights, the wrappers around the controls) was a dead spot. Buttons, tabs
  // and menus still take their clicks: Tauri never drags from a clickable
  // element. Double-click to maximize comes with it, from Tauri's handler.

  let {
    title = 'Stroke',
    connected = false,
    aiSidebarOpen = false,
    /** Full-window AI mode is on: the chat button shows it, and a click folds it back into the side panel. */
    aiMode = false,
    ontoggleaisidebar = () => {},
    /** Named actions for the menu bar - see AppMenuBar. */
    menuActions = /** @type {Record<string, () => void>} */ ({}),
    // macOS title row: sidebar toggle + breadcrumb.
    sidebarOpen = false,
    /** @type {{ workspaceId?: string | null, color?: string | null } | null} */
    savedConnection = null,
    /** @type {import('$lib/studio-tabs.js').StudioTab | null} */
    activeTab = null,
    /** Every saved connection, for the workspace switcher's counts. @type {{ workspaceId?: string | null }[]} */
    connections = [],
    /** The sidebar's drawn width in screen px (0 when hidden), for the full-height sidebar look. */
    sidebarWidth = 0,
    /** The open tabs, rendered in this row (single pane). @type {import('svelte').Snippet | undefined} */
    tabStrip = undefined,
  } = $props()

  // macOS: the menus live in the native menu bar (src-tauri/src/mac_chrome.rs),
  // which runs the same `menuActions` by name.
  let fullscreen = $state(false)
  onMount(() => {
    if (!isMac || !('__TAURI_INTERNALS__' in window)) return
    const unbindMenu = bindNativeMenu(() => menuActions, () => connected)
    const unwatch = watchFullscreen((v) => (fullscreen = v))
    return () => { unbindMenu(); unwatch() }
  })
  $effect(() => {
    if (isMac && isTauri) void syncNativeMenuState(connected)
  })

  const pageTitle = $derived(connected && activeTab ? tabDisplayTitle(activeTab) : '')


  const trialDays = $derived(
    $licenseStatus?.status === 'Trial' ? $licenseStatus.days_remaining : 0
  )
  const trialUrgent = $derived(trialDays <= 3)

  // The breadcrumb is centred on the whole bar, so its room is twice the
  // smaller of the two gaps between the centre and the controls on either side,
  // less a margin. Measured rather than set by a breakpoint: the controls grow
  // with the app zoom and a viewport media query does not, which is what left
  // a few px of trail crushed between the menus and the workspace switcher.
  let barW = $state(0)
  let leftW = $state(0)
  let rightW = $state(0)
  const BAR_PAD = 10
  const CRUMB_GAP = 16
  const crumbRoom = $derived(
    Math.max(0, 2 * (Math.min(barW / 2 - BAR_PAD - leftW, barW / 2 - BAR_PAD - rightW) - CRUMB_GAP)),
  )
  // Windows/Linux centre title: same room, shown whole when it fits, truncated
  // down to a few characters' worth, and hidden below that rather than shown
  // as a stub. Its natural width comes from an invisible copy.
  let titleW = $state(0)
  const showTitle = $derived(crumbRoom > 0 && crumbRoom >= Math.min(titleW, 96))

  // The bar zooms with the rest of the UI. It used to be counter-zoomed to stay
  // at 100% like an OS title bar, but at 200% that left the menus, workspace
  // switcher and (on macOS) the open tabs at half the size of everything below
  // them. Its height is --app-titlebar-h (StudioShell), which scales too, so the
  // sidebar's top row and the inset pages stay level with it. Inside the bar the
  // spacing unit is 4px x scale, the size it always had at 100%.

  const iconBtn = 'inline-flex size-[calc(26px*var(--app-scale))] items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground active:bg-foreground/[0.1]'
</script>

<!--
  The window is frameless (see src-tauri/src/lib.rs), so this bar IS the title
  bar: it's the drag region and, on Windows/Linux, draws its own minimize /
  maximize / close. macOS keeps the real OS-drawn traffic lights (Overlay
  style) floating over this bar, so it only reserves space for them here.
-->
<div
  class="studio-chrome relative flex shrink-0 items-center border-b border-border/50 bg-panel h-(--app-titlebar-h) {isMac ? 'pr-[calc(10px*var(--app-scale))]' : 'pl-[calc(10px*var(--app-scale))] pr-0'} select-none"
  data-studio-chrome
  style:--spacing="calc(4px * var(--app-scale, 1))"
  data-studio-region="titlebar"
  bind:clientWidth={barW}
  data-tauri-drag-region="deep"
  role="toolbar"
  aria-label="Window title bar"
  tabindex="-1"
>

  {#if isMac}
    <!-- Room for the native traffic lights (inset set in mac_chrome.rs: they end
         ~66px in). Full screen hides them, so the room goes too. -->
    <!-- The lights sit over the sidebar's top row when it is open; with it
         hidden they are over this bar, so it leaves them room. -->
    <div class="flex shrink-0 items-center self-stretch" bind:clientWidth={leftW}>
      <!-- 78px at every zoom: the OS draws the lights at a fixed size. -->
      {#if !sidebarWidth && !fullscreen}<div class="w-[78px] shrink-0"></div>{/if}
    </div>

    <!-- The content's top row: where you are, left-aligned from the sidebar's
         edge. A hairline under this part only, so the sidebar column above runs
         unbroken. No in-window File/Edit/View on macOS: those menus live in the
         system menu bar (mac_chrome.rs), where Mac apps keep them. -->
    {#if tabStrip}
      <!-- Never wider than the room between the bar's edges: many tabs scroll
           inside the strip instead of running under the controls on the right. -->
      <div class="titlebar-tabstrip flex h-full min-w-0 flex-1 items-center overflow-hidden">{@render tabStrip()}</div>
    {/if}
    <div class="pointer-events-none flex min-w-0 items-center pl-[calc(14px*var(--app-scale))]">
      <!-- Not connected, it would only repeat the workspace the switcher on the
           right already names. -->

    </div>
  {:else}
    <div class="flex min-w-0 items-center" bind:clientWidth={leftW}>
    <!-- Stroke mark -->
    <Logo class="mr-2 size-4 shrink-0" />

    <!-- Menu bar. First after the mark, which is where every desktop app puts it
         and where people look before they look for a shortcut. Optional: everything
         in it is also in ⌘K and on a shortcut, so hiding it costs nothing but the
         menus themselves. -->
    {#if $appMenuBar}
      <AppMenuBar {connected} actions={menuActions} />
    {/if}
    </div>

    <!-- Center title. Held to the measured room between the controls (a fixed
         px-40 grew with the zoom and ran the title under the menus), truncating
         a long label and hiding when only a stub would fit. -->
    {@const titleCls = 'truncate text-ui-xs font-medium text-muted-foreground select-none'}
    <div class="pointer-events-none absolute inset-x-0 flex items-center justify-center">
      <span class={cn('invisible absolute top-0 left-0 w-max', titleCls)} aria-hidden="true" bind:clientWidth={titleW}>{title}</span>
      {#if showTitle}
        <span class={titleCls} style:max-width="{crumbRoom}px">{title}</span>
      {/if}
    </div>
  {/if}

  <!-- Spacer (also draggable - it's the largest empty stretch of the bar) -->
  <!-- Only macOS draws the tab strip here; elsewhere the spacer has to stretch,
       or the controls on the right ended up beside the menus. -->
  <div class={isMac && tabStrip ? 'w-2 shrink-0' : 'min-w-0 flex-1'} data-tauri-drag-region></div>

  <!-- Right: trial pill + Chat -->
  <div class="flex shrink-0 items-center gap-[calc(4px*var(--app-scale))] self-stretch" bind:clientWidth={rightW}>

    <!-- Workspace, then the app menu (account, settings, theme, help). -->
    <WorkspaceSwitcher {connections} compact />
    <TitleBarMenu actions={menuActions} class="size-[calc(28px*var(--app-scale))]" />

    {#if isTauri && !isMac}
      <!-- Windows-style caption buttons: full height, flush with the window's edge. -->
      <div class="ml-2 flex self-stretch">
        <WindowControls />
      </div>
    {/if}
  </div>
</div>

