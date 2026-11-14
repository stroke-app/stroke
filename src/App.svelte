<script>
  import { onMount } from 'svelte'
  import { Toaster } from '$lib/components/ui/sonner/index.js'
  import StudioShell from './lib/components/StudioShell.svelte'
  import LicenseGate from './lib/components/LicenseGate.svelte'
  import AppLockGate from './lib/components/AppLockGate.svelte'
  import AppErrorBoundary from './lib/components/AppErrorBoundary.svelte'
  import CrashNow from './lib/components/CrashNow.svelte'
  import { crashArmed } from './lib/games/easter-eggs.js'
  import GlobalTooltip from './lib/components/GlobalTooltip.svelte'
  import PerfOverlay from './lib/components/PerfOverlay.svelte'
  import { loadSettings, applySettings, installZoomShortcuts } from '$lib/stores/settings.js'
  import { installPlatformClass } from '$lib/platform.js'
  import { installClipboardBridge } from '$lib/clipboard.js'
  import { installShiftWheel } from '$lib/shift-wheel.js'

  onMount(async () => {
    installPlatformClass()
    applySettings(loadSettings())
    installZoomShortcuts()
    // Shift + wheel scrolls sideways in every scroll area, whatever the webview does.
    installShiftWheel()
    // Route clipboard through Tauri's native plugin so copy works in the WKWebview
    // (navigator.clipboard is denied there, incl. Monaco's internal copy).
    void installClipboardBridge()

    // Dev-only console helpers (window.__TAURI__ isn't exposed without
    // withGlobalTauri, which we keep off in production). Debug trial commands
    // only exist in debug builds, so these are dev-only too.
    if (import.meta.env.DEV) {
      const { invoke } = await import('@tauri-apps/api/core')
      const { refreshLicenseStatus } = await import('$lib/stores/license.js')
      // @ts-expect-error - dev-only debug surface
      window.__stroke = {
        status: () => invoke('check_license_status'),
        resetTrial: async () => { await invoke('debug_reset_trial'); return refreshLicenseStatus() },
        expireTrial: async (daysAgo = 20) => { await invoke('debug_set_trial_days_ago', { daysAgo }); return refreshLicenseStatus() },
      }
      console.info('[stroke] dev helpers → window.__stroke.status() / .expireTrial(20) / .resetTrial()')
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
  })
</script>

<Toaster position="top-right" offset={{ top: '52px', right: '12px' }} closeButton />
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
      <StudioShell />
    </LicenseGate>
  </AppLockGate>
</AppErrorBoundary>
<GlobalTooltip />
<!-- Frame-rate HUD, dev builds only. F8 toggles it and the choice sticks. -->
{#if import.meta.env.DEV}<PerfOverlay />{/if}
