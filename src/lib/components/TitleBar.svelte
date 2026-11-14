<script>
  import DbIcon from './DbIcon.svelte'
  import { onMount } from 'svelte'
  import MessageSquare from '@lucide/svelte/icons/message-square'
  import X             from '@lucide/svelte/icons/x'
  import KeyRound      from '@lucide/svelte/icons/key-round'
  import { cn } from '$lib/utils.js'
  import { getAppScale } from '$lib/app-zoom.js'
  import { detectOs } from '$lib/platform.js'
  import { isTrialActive, licenseStatus } from '$lib/stores/license.js'
  import LicenseActivation from './LicenseActivation.svelte'
  import Logo from './Logo.svelte'
  import AppMenuBar from './AppMenuBar.svelte'
  import { appMenuBar, appZoom } from '$lib/stores/settings.js'
  import WindowControls from './WindowControls.svelte'
  import TitleBarMenu from './TitleBarMenu.svelte'
  import WorkspaceSwitcher from './WorkspaceSwitcher.svelte'
  import PanelLeft from '@lucide/svelte/icons/panel-left'
  import { tabDisplayTitle } from '$lib/studio-tabs.js'
  import { bindNativeMenu, syncNativeMenuState, watchFullscreen } from '$lib/native-menu.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'

  const isMac = typeof navigator !== 'undefined' && detectOs() === 'macos'
  const mod = isMac ? '⌘' : 'Ctrl'

  // Window is frameless everywhere (see src-tauri/src/lib.rs), so this bar is
  // the drag region and double-click-to-maximize target.
  let isTauri = $state(false)
  onMount(() => {
    isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  })

  // Native title bars maximize on a background double-click. Our drag region
  // only covers elements tagged `data-tauri-drag-region`; Tauri's own injected
  // handler already maximizes when the click lands directly on one of those -
  // this only picks up the rest (e.g. the flex spacer's non-tagged padding).
  /** @param {MouseEvent} e */
  function onDragRegionDblClick(e) {
    // macOS tags the whole row `deep`, so Tauri's own handler already zooms on
    // a double-click anywhere in it; running this as well would undo it.
    if (!isTauri || isMac) return
    if (e.target === e.currentTarget) return
    if (/** @type {Element} */ (e.target).closest('button')) return
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().toggleMaximize())
  }

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

  let showActivationDialog = $state(false)

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

  // Fixed px like the bar itself: the macOS bar is a fixed 38px, so a rem
  // control (42px at 150%) overflowed it. One 28px row at every zoom.
  // The title bar never zooms, like the OS's own: the traffic lights beside it
  // are drawn at 100% whatever the app zoom, so a bar that grew with the UI
  // overflowed them (menus, workspace name and buttons all ballooned). It is
  // counter-zoomed by 1/scale; its few px sizes are written × scale above so
  // they land back on whole pixels. Menus it opens are portaled and still zoom.
  // Read in the same frame: applySettings sets --app-scale before it updates
  // appZoom. Deferring it a frame made the bar jump after every zoom step.
  const chromeZoom = $derived.by(() => {
    void $appZoom
    return 1 / getAppScale()
  })

  const iconBtn = 'inline-flex size-[calc(28px*var(--app-scale))] items-center justify-center rounded-md text-muted-foreground hover:text-foreground'
</script>

<!--
  The window is frameless (see src-tauri/src/lib.rs), so this bar IS the title
  bar: it's the drag region and, on Windows/Linux, draws its own minimize /
  maximize / close. macOS keeps the real OS-drawn traffic lights (Overlay
  style) floating over this bar, so it only reserves space for them here.
-->
<div
  class="studio-chrome relative flex h-[calc(52px*var(--app-scale))] shrink-0 items-center bg-panel {isMac ? 'pr-[calc(10px*var(--app-scale))]' : 'border-b border-border/40 px-[calc(10px*var(--app-scale))]'} select-none"
  data-studio-chrome
  style:zoom={chromeZoom}
  data-studio-region="titlebar"
  bind:clientWidth={barW}
  data-tauri-drag-region
  role="toolbar"
  aria-label="Window title bar"
  tabindex="-1"
  ondblclick={onDragRegionDblClick}
>

  {#if isMac}
    <!-- Room for the native traffic lights (inset set in mac_chrome.rs: they end
         ~68px in). Full screen hides them, so the room goes too. -->
    <!-- Pixels, not rem: the traffic lights are drawn by the OS and ignore the
         app's zoom (which scales the root font size), so anything sized in rem
         here drifted toward or away from them as you zoomed. The lights end at
         ~66px; the bar's own 10px padding plus this spacer leaves a steady 12px
         before the toggle. -->
    <!-- The sidebar's top: same width and surface as the sidebar under it, so
         the sidebar reads as one full-height column (as in T3 Code). Collapsed,
         it is just the lights and the toggle, and the rest slides left. -->
    <!-- The lights sit over the sidebar's top row when it is open; with it
         hidden they are over this bar, so it leaves them room. -->
    <div class="flex shrink-0 items-center self-stretch" bind:clientWidth={leftW}>
      {#if !sidebarWidth && !fullscreen}<div class="w-[calc(92px*var(--app-scale))] shrink-0"></div>{/if}
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
  <div class="flex shrink-0 items-center gap-[calc(4px*var(--app-scale))]" bind:clientWidth={rightW}>

    <!-- Workspace, then the app menu (account, settings, theme, help). -->
    <WorkspaceSwitcher {connections} compact />
    <TitleBarMenu actions={menuActions} class="size-[calc(28px*var(--app-scale))]" />

    {#if isTauri && !isMac}
      <div class="ml-1 border-l border-border/40 pl-1.5">
        <WindowControls />
      </div>
    {/if}
  </div>
</div>

<!-- License activation dialog (from trial pill) -->
<Dialog.Root bind:open={showActivationDialog}>
  <Dialog.Portal>
    <Dialog.Overlay class="fixed inset-0 z-[70] bg-black/65" />
    <Dialog.Content showCloseButton={false} class="fixed left-1/2 top-1/2 z-[71] w-full max-w-[420px] -translate-x-1/2 -translate-y-1/2 overflow-hidden rounded-2xl border border-border/60 bg-background p-0 elevate-3-rim outline-none">
      <div class="flex items-center justify-between border-b border-border/60 px-5 py-4">
        <div class="flex items-center gap-2.5">
          <div class="flex size-7 items-center justify-center rounded-lg bg-primary/10">
            <KeyRound class="size-3.5 shrink-0 text-primary" />
          </div>
          <div>
            <Dialog.Title class="text-ui-sm font-semibold leading-none text-foreground">Activate license</Dialog.Title>
            <Dialog.Description class="mt-0.5 text-ui-xs text-muted-foreground">Enter your key to unlock all features.</Dialog.Description>
          </div>
        </div>
        <Dialog.Close class="inline-flex size-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent hover:text-foreground">
          <X class="size-4 shrink-0" />
        </Dialog.Close>
      </div>
      <LicenseActivation compact onactivated={() => { showActivationDialog = false }} />
      <div class="border-t border-border/60 px-5 py-3">
        <p class="text-ui-xs text-muted-foreground">
          No license? <a href="https://stroke.click" target="_blank" rel="noopener noreferrer" class="text-primary underline-offset-2 hover:underline">stroke.click →</a>
        </p>
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
