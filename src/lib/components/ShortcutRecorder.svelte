<script>
  import { Button } from '$lib/components/ui/button/index.js'
  /**
   * One command's key, editable in place: the keycaps, a pencil to record a new
   * combo, a reset when it differs from the default, and an inline Replace /
   * Cancel when the combo is taken. Same rules as Settings ▸ Keybindings: a bare
   * modifier records nothing, Esc cancels, ⌫ unbinds, and the keys being typed
   * never reach the app (capture-phase listener, stopped there).
   */
  import { onDestroy, tick } from 'svelte'
  import Pencil from '@lucide/svelte/icons/pencil'
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw'
  import Kbd from './Kbd.svelte'
  import { cn } from '$lib/utils.js'
  import { keycaps, comboTitle } from '$lib/shortcuts.js'
  import {
    keybindings,
    effectiveCombo,
    isOverridden,
    setBinding,
    unbind,
    resetBinding,
    assignReplacing,
    findConflict,
    recordKeydown,
    setRecordingShortcut,
  } from '$lib/stores/keybindings.js'

  /** @type {{ id: string, label: string, size?: 'sm' | 'md' }} */
  let { id, label, size = 'md' } = $props()

  const combo = $derived(effectiveCombo($keybindings, id))
  const changed = $derived(isOverridden($keybindings, id))

  let recording = $state(false)
  /** @type {{ combo: string, conflict: import('$lib/stores/keybindings.js').Conflict } | null} */
  let pending = $state(null)
  /** @type {HTMLButtonElement | null} */
  let editBtn = $state(null)

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    e.preventDefault()
    e.stopPropagation()
    e.stopImmediatePropagation()
    if (e.repeat) return
    const result = recordKeydown(e)
    if (!result) return
    stop()
    if (result.type === 'cancel') return
    if (result.type === 'clear') return unbind(id)
    if (result.combo === combo) return
    const conflict = findConflict($keybindings, result.combo, id)
    if (conflict) pending = { combo: result.combo, conflict }
    else setBinding(id, result.combo)
  }

  function start() {
    pending = null
    recording = true
    setRecordingShortcut(true)
    window.addEventListener('keydown', onKey, true)
  }

  function stop() {
    recording = false
    setRecordingShortcut(false)
    window.removeEventListener('keydown', onKey, true)
    void tick().then(() => editBtn?.focus())
  }

  function confirm() {
    if (!pending) return
    if (pending.conflict.fixed) setBinding(id, pending.combo)
    else assignReplacing(id, pending.combo, pending.conflict.id)
    pending = null
  }

  onDestroy(() => {
    if (recording) {
      setRecordingShortcut(false)
      window.removeEventListener('keydown', onKey, true)
    }
  })

  const btn = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground'
</script>

<div class="flex shrink-0 items-center gap-1">
  {#if pending}
    <span class="max-w-72 truncate text-ui-2xs text-muted-foreground" title={`${comboTitle(pending.combo)} is used by “${pending.conflict.label}”`} role="alert">
      {comboTitle(pending.combo)} is used by “{pending.conflict.label}”
    </span>
    <button type="button" class="h-7 rounded-md px-2 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={() => (pending = null)}>Cancel</button>
    <Button size="sm" onclick={confirm}>
      {pending.conflict.fixed ? 'Assign anyway' : 'Replace'}
    </Button>
  {:else if recording}
    <span class="rounded-md bg-accent px-2 py-1 text-ui-2xs text-foreground" aria-live="assertive">Press keys · Esc cancels · ⌫ unbinds</span>
  {:else if combo}
    <Kbd keys={keycaps(combo)} {size} class="gap-1" />
  {:else}
    <span class="text-ui-2xs text-muted-foreground">Unbound</span>
  {/if}

  {#if !pending}
    <button
      bind:this={editBtn}
      type="button"
      class={cn(btn, 'ml-1', recording ? 'bg-accent text-foreground' : 'opacity-0 group-hover/row:opacity-100 focus-visible:opacity-100')}
      aria-label={recording ? `Stop recording a shortcut for ${label}` : `Change the shortcut for ${label}`}
      aria-pressed={recording}
      title={recording ? 'Recording, Esc cancels' : 'Change shortcut'}
      onclick={() => (recording ? stop() : start())}
    >
      <Pencil class="size-3.5 shrink-0" />
    </button>
    {#if changed && !recording}
      <button type="button" class={btn} aria-label={`Reset the shortcut for ${label}`} title="Reset to default" onclick={() => resetBinding(id)}>
        <RotateCcw class="size-3.5 shrink-0" />
      </button>
    {/if}
  {/if}
</div>
