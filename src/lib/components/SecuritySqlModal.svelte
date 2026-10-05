<script>
  import { onMount } from 'svelte'
  import { completionStatus } from '@codemirror/autocomplete'
  import CodeEditor from './CodeEditor.svelte'
  import X from '@lucide/svelte/icons/x'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Play from '@lucide/svelte/icons/play'
  import { focusTrap } from '$lib/actions/focus-trap.js'

  let {
    title = '',
    sql = $bindable(''),
    running = false,
    onclose = () => {},
    onrun = () => {},
  } = $props()

  /** @type {CodeEditor | null} */
  let editor = $state(null)

  /** ⌘↵ runs, Esc closes - unless the suggestion list is open, which Esc closes first. */
  const keys = [
    { key: 'Mod-Enter', run: () => { if (!running) onrun(); return true } },
    {
      key: 'Escape',
      run: (/** @type {import('@codemirror/view').EditorView} */ view) => {
        if (completionStatus(view.state)) return false
        if (!running) onclose()
        return true
      },
    },
  ]

  // Focus the editor when the modal opens.
  onMount(() => editor?.focus())
</script>

<!-- Backdrop -->
<!-- svelte-ignore a11y_interactive_supports_focus a11y_click_events_have_key_events -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/65"
  role="dialog"
  aria-modal="true"
  tabindex="-1"
  onclick={(e) => { if (e.target === e.currentTarget && !running) onclose() }}
  onkeydown={(e) => { if (e.key === 'Escape' && !running) onclose() }}
  use:focusTrap={{ autoFocus: false }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div
    role="presentation"
    class="flex w-[40rem] flex-col overflow-hidden rounded-2xl border border-border/60 bg-background elevate-3-rim"
    style="height: 420px"
    onclick={(e) => e.stopPropagation()}
  >
    <!-- Header -->
    <div class="flex shrink-0 items-center justify-between border-b border-border/50 px-4 py-2.5">
      <span class="font-mono text-ui-sm font-medium text-foreground">{title}</span>
      <button
        type="button"
        class="inline-flex size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        onclick={onclose}
        disabled={running}
      >
        <X class="size-3.5" />
      </button>
    </div>

    <!-- The editor fills the space -->
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
      <CodeEditor bind:this={editor} bind:value={sql} lang="sql" dialect="postgres" {keys} ariaLabel={title || 'SQL'} />
    </div>

    <!-- Footer -->
    <div class="flex shrink-0 items-center justify-between border-t border-border/50 px-4 py-2.5">
      <p class="font-mono text-ui-2xs text-muted-foreground">
        <kbd>⌘↵</kbd> to run
        &nbsp;·&nbsp;
        <kbd>Esc</kbd> to close
      </p>
      <div class="flex items-center gap-2">
        <button
          type="button"
          class= "field-surface inline-flex h-7 items-center px-3 font-mono text-ui-xs text-muted-foreground transition-colors hover:bg-muted"
          onclick={onclose}
          disabled={running}
        >Cancel</button>
        <button
          type="button"
          class="inline-flex h-7 items-center gap-1.5 rounded-md bg-primary px-3 font-mono text-ui-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
          onclick={onrun}
          disabled={running || !sql.trim()}
        >
          {#if running}
            <RefreshCw class="size-3.5 shrink-0 animate-spin" />Running…
          {:else}
            <Play class="size-3.5 shrink-0" />Run SQL
          {/if}
        </button>
      </div>
    </div>
  </div>
</div>
