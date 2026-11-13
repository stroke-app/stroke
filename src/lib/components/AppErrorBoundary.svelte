<script>
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw'
  import Bug from '@lucide/svelte/icons/bug'
  // Static: StudioShell already imports it, so it is in the main chunk either way.
  import ReportIssueDialog from './ReportIssueDialog.svelte'
  import { revealApp } from '$lib/app-reveal.js'

  let { children } = $props()

  // Report dialog state lives on the boundary itself (not the failed subtree),
  // so it survives whatever blew up below.
  let reportOpen = $state(false)
  let reportTitle = $state('')
  let reportDetails = $state('')

  /** @param {unknown} error */
  function openReport(error) {
    const err = /** @type {any} */ (error)
    const message = String(err?.message ?? err ?? 'Unknown error')
    reportTitle = `[crash] ${message.slice(0, 120)}`
    // Stack when the runtime gave us one, message otherwise. Capped so the
    // GitHub URL stays under the length browsers and GitHub accept.
    reportDetails = String(err?.stack || message).slice(0, 2500)
    reportOpen = true
  }
</script>

<svelte:boundary onerror={(e) => { try { console.error('[app-boundary]', e) } catch { /* noop */ } revealApp() }}>
  {@render children()}

  {#snippet failed(error, reset)}
    <div class="flex h-screen w-screen flex-col items-center justify-center gap-5 bg-background px-8 text-center select-none">
      <div class="flex size-12 items-center justify-center rounded-lg bg-destructive/10">
        <TriangleAlert class="size-6 shrink-0 text-destructive" />
      </div>

      <div class="space-y-1">
        <h1 class="text-ui font-semibold text-foreground">Something went wrong</h1>
        <p class="mx-auto max-w-md text-ui-sm text-muted-foreground">
          The interface hit an unexpected error. Try to recover, or reload, your
          connections and data are safe.
        </p>
      </div>

      <pre class="max-h-40 w-full max-w-lg overflow-auto rounded-lg border border-border/60 bg-muted/30 p-3 text-left font-mono text-ui-2xs leading-relaxed text-muted-foreground select-text">{String(/** @type {any} */ (error)?.message ?? error)}</pre>

      <div class="flex items-center gap-2">
        <button
          type="button"
          onclick={reset}
          class="inline-flex h-8 items-center gap-1.5 rounded-md bg-foreground px-3 text-ui-xs font-medium text-background transition-opacity hover:opacity-85"
        >
          <RotateCcw class="size-3.5 shrink-0" /> Try to recover
        </button>
        <button
          type="button"
          onclick={() => location.reload()}
          class= "field-surface inline-flex h-8 items-center px-3 text-ui-xs font-medium text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
        >
          Reload app
        </button>
        <button
          type="button"
          onclick={() => openReport(error)}
          class= "field-surface inline-flex h-8 items-center gap-1.5 px-3 text-ui-xs font-medium text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
        >
          <Bug class="size-3.5 shrink-0" /> Report issue
        </button>
      </div>
    </div>
  {/snippet}
</svelte:boundary>

<!-- Mounted only after a crash, so the happy path never builds the dialog. -->
{#if reportOpen}
  <ReportIssueDialog
    bind:open={reportOpen}
    initialType="crash"
    initialTitle={reportTitle}
    initialBody="The app hit an unexpected error and showed the recovery screen."
    details={reportDetails}
  />
{/if}
