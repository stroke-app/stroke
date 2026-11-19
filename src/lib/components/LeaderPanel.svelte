<script>
  /**
   * The leader key's second step. Press the leader (⌘; / Ctrl+;) and this
   * lists what can follow; the next key runs it. Anything else, Escape, or a
   * few seconds of nothing, closes it. It shows every follow-up so nothing has
   * to be memorised, and the letters are the words' first letters (S for SQL,
   * A for AI, T for terminal) so they stick after a few uses.
   */
  import { onDestroy } from 'svelte'
  import Kbd from './Kbd.svelte'

  /**
   * @typedef {{ key: string, label: string, run: () => void }} LeaderAction
   */
  let {
    open = $bindable(false),
    /** @type {LeaderAction[]} */
    actions = [],
  } = $props()

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer

  /** Number keys collapse into one row ("1-9 Go to tab"). */
  const rows = $derived.by(() => {
    const digits = actions.filter((a) => /^[1-9]$/.test(a.key))
    const rest = actions.filter((a) => !/^[1-9]$/.test(a.key))
    return [...(digits.length ? [{ key: '1-9', label: 'Go to tab (9 = last)' }] : []), ...rest]
  })

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (!open) return
    // Modifiers alone (the leader's own key being released) are not the answer.
    if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return
    e.preventDefault()
    e.stopPropagation()
    e.stopImmediatePropagation()
    open = false
    if (e.key === 'Escape') return
    const hit = actions.find((a) => a.key.toLowerCase() === e.key.toLowerCase())
    hit?.run()
  }

  $effect(() => {
    if (!open) return
    clearTimeout(timer)
    timer = setTimeout(() => (open = false), 4000)
    window.addEventListener('keydown', onKey, true)
    return () => {
      clearTimeout(timer)
      window.removeEventListener('keydown', onKey, true)
    }
  })
  onDestroy(() => clearTimeout(timer))
</script>

{#if open}
  <!-- Bottom-centre, above the status bar: where the eye already is when the
       hands are on the keyboard. No animation: it is keyboard-initiated. -->
  <div
    class="pointer-events-none fixed inset-x-0 z-[80] flex justify-center px-4"
    style="bottom: calc(var(--app-statusbar-h, 0px) + 16px)"
    role="dialog"
    aria-label="Leader key: press a key"
  >
    <div class="pointer-events-auto w-full max-w-xl rounded-lg border border-border/60 bg-popover p-3 elevate-3-rim">
      <p class="mb-2 px-1 text-ui-2xs font-medium text-muted-foreground">Then press</p>
      <ul class="grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-3">
        {#each rows as r (r.key)}
          <li class="flex min-w-0 items-center gap-2 rounded-md px-1 py-0.5">
            <Kbd keys={[r.key]} size="sm" />
            <span class="min-w-0 truncate text-ui-xs text-foreground/90">{r.label}</span>
          </li>
        {/each}
      </ul>
    </div>
  </div>
{/if}
