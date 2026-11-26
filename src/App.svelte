<script>
  import { toastPosition } from '$lib/stores/toast-position.js'
  import { onMount } from 'svelte'
  import { Toaster } from '$lib/components/ui/sonner/index.js'
  import ZoomIndicator from '$lib/components/ZoomIndicator.svelte'
  import StudioShell from './lib/components/StudioShell.svelte'


  // A window opened for one dashboard (open_dashboard_window) shows only that.
  const params = typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams()
  const dashboardView = params.get('view') === 'dashboard' ? { dash: params.get('dash') ?? '', conn: params.get('conn') ?? '' } : null
  // The Local servers window (open_local_servers_window).
  const localServersFrom = params.get('view') === 'local-servers' ? params.get('from') || 'main' : null
  import LicenseGate from './lib/components/LicenseGate.svelte'
  import AppLockGate from './lib/components/AppLockGate.svelte'
  import AppErrorBoundary from './lib/components/AppErrorBoundary.svelte'
  import CrashNow from './lib/components/CrashNow.svelte'
  import { crashArmed } from './lib/games/easter-eggs.js'
  import GlobalTooltip from './lib/components/GlobalTooltip.svelte'
  import ConfirmHost from './lib/components/ConfirmHost.svelte'
  import PerfOverlay from './lib/components/PerfOverlay.svelte'
  import { loadSettings, applySettings, installZoomShortcuts } from '$lib/stores/settings.js'
  import { installPlatformClass, installWindowStateListeners } from '$lib/platform.js'
  import { installClipboardBridge } from '$lib/clipboard.js'
  import { trackWindowResize } from '$lib/panel-motion.js'
  import { installTitleBarDoubleClick } from '$lib/titlebar-double-click.js'
  import { installShiftWheel } from '$lib/shift-wheel.js'
  import { installSlashFocus } from '$lib/slash-focus.js'

  onMount(() => {
    installPlatformClass()
    const unwatchWindow = installWindowStateListeners()
    applySettings(loadSettings())
    installZoomShortcuts()
    // A window resize burst (macOS zoom, edge drag) counts as a panel drag.
    trackWindowResize()
    // macOS: a title-bar double-click follows the Desktop & Dock setting.
    installTitleBarDoubleClick()
    // Shift + wheel scrolls sideways in every scroll area, whatever the webview does.
    installShiftWheel()
    // `/` focuses the search field in view.
    installSlashFocus()
    // Route clipboard through Tauri's native plugin so copy works in the WKWebview
    // (navigator.clipboard is denied there, incl. Monaco's internal copy).
    void installClipboardBridge()

    // Dev-only console helpers (window.__TAURI__ isn't exposed without
    // withGlobalTauri, which we keep off in production).
    if (import.meta.env.DEV) {
      void (async () => {
        const { invoke } = await import('@tauri-apps/api/core')
        const { refreshLicenseStatus } = await import('$lib/stores/license.js')
        // @ts-expect-error - dev-only debug surface
        window.__stroke = {
          status: () => invoke('check_license_status'),
          refresh: () => refreshLicenseStatus(),
        }
        console.info('[stroke] dev helpers → window.__stroke.status() / .refresh()')
      })()
    }

    // Suppress Tauri's internal "Couldn't find callback id" warning.
    // This is harmless noise that fires when Rust resolves a promise after a
    // hot-reload or fast navigation has already torn down the JS callback.
    const _warn = console.warn
    console.warn = (/** @type {unknown} */ ...args) => {
      if (typeof args[0] === 'string' && args[0].includes("Couldn't find callback id")) return
      _warn.apply(console, args)
    }

    // Swallow the benign "ResizeObserver loop …" error. It fires when an
    // observer callback changes layout within the same frame (e.g. toggling
    // the Advanced disclosure or read-only row resizes a ScrollArea in the
    // connection dialog). It's safe to ignore per the ResizeObserver spec, but
    // WebKitGTK raises it as an uncaught error and Vite paints its dev overlay.
    // Silence only this one message, at both the event and console layers.
    const isResizeObserverLoop = (/** @type {unknown} */ msg) =>
      typeof msg === 'string' && msg.includes('ResizeObserver loop')
    window.addEventListener('error', (e) => {
      if (isResizeObserverLoop(e.message)) { e.stopImmediatePropagation(); e.preventDefault() }
    }, true)
    const _error = console.error
    console.error = (/** @type {unknown} */ ...args) => {
      if (isResizeObserverLoop(args[0])) return
      _error.apply(console, args)
    }

    // Block print - no Tauri-level API exists for this, so override at the JS boundary
    window.print = () => {}
    window.addEventListener('beforeprint', (e) => e.preventDefault(), { capture: true })
    document.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'p') { e.preventDefault(); e.stopPropagation() }
    }, { capture: true })

    // No fade-in here: AppLockGate has not rendered anything yet. The first
    // complete screen reveals the page - see $lib/app-reveal.js.
    return () => unwatchWindow()
  })
</script>

<!-- The corner is a setting (Appearance → Notifications). Top corners clear
     the titlebar, bottom ones the status bar. -->
<ZoomIndicator />
<Toaster
  position={$toastPosition}
  offset={{ top: '42px', bottom: '40px', left: '16px', right: '16px' }}
  closeButton
/>
<AppErrorBoundary>
  <!-- Mounts only when armed, throws on mount, and disarms itself first. Inside
       the boundary so the crash screen catches it; a sibling of the app rather
       than a child of it so the throw does not depend on anything below
       rendering successfully. -->
  {#if $crashArmed}<CrashNow />{/if}
  <!-- The PIN sits outside the license gate: if the app is locked, nothing
       behind it runs - not the license check, not a reconnect. -->
  <AppLockGate>
    <LicenseGate>
      {#if dashboardView}
        {#await import('./lib/components/DashboardWindow.svelte') then { default: DashboardWindow }}<DashboardWindow dashboardId={dashboardView.dash} connectionId={dashboardView.conn} />{/await}
      {:else if localServersFrom}
        {#await import('./lib/components/LocalServersWindow.svelte') then { default: LocalServersWindow }}
          <LocalServersWindow from={localServersFrom} />
        {/await}
      {:else}
        <StudioShell />
      {/if}
    </LicenseGate>
  </AppLockGate>
</AppErrorBoundary>
<GlobalTooltip />
<ConfirmHost />
<!-- Frame-rate HUD, dev builds only. F8 toggles it and the choice sticks. -->
{#if import.meta.env.DEV}<PerfOverlay />{/if}
