<script>
  /**
   * Editor theme gallery for Settings: one card per preset, each showing the
   * same few lines of SQL in that preset's real colours (plain spans, not an
   * editor per card). A radio group: arrow keys move the choice, Tab leaves.
   */
  import Check from '@lucide/svelte/icons/check'
  import { cn } from '$lib/utils.js'
  import { EDITOR_THEMES, EDITOR_THEME_IDS, editorPreviewColors } from '$lib/themes/editor-themes.js'

  let {
    /** @type {import('$lib/themes/editor-themes.js').EditorThemeId} */
    value = 'app',
    onpick = /** @type {(id: import('$lib/themes/editor-themes.js').EditorThemeId) => void} */ (() => {}),
  } = $props()

  /** @typedef {'comment' | 'keyword' | 'punctuation' | 'variable' | 'operator' | 'string'} Part */
  /**
   * Coloured the way the SQL editor colours it: lang-sql reads `sum` as a
   * plain name, not a function, so the card does too.
   * @type {[string, Part][][]}
   */
  const SAMPLE = [
    [['-- paid orders', 'comment']],
    [['SELECT', 'keyword'], [' sum', 'variable'], ['(', 'punctuation'], ['total', 'variable'], [')', 'punctuation']],
    [['FROM', 'keyword'], [' orders', 'variable']],
    [['WHERE', 'keyword'], [' kind ', 'variable'], ['=', 'operator'], [' ', 'variable'], ["'gift'", 'string'], [';', 'punctuation']],
  ]

  /** @type {HTMLDivElement | null} */
  let group = $state(null)

  /** Arrow keys step through the presets and pick, as in any radio group. */
  function onkeydown(/** @type {KeyboardEvent} */ e) {
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key]
    if (!step) return
    e.preventDefault()
    const at = EDITOR_THEME_IDS.indexOf(value)
    const next = EDITOR_THEME_IDS[(at + step + EDITOR_THEME_IDS.length) % EDITOR_THEME_IDS.length]
    onpick(next)
    group?.querySelector(`[data-theme-id="${next}"]`)?.focus()
  }
</script>

<div
  bind:this={group}
  role="radiogroup"
  aria-label="Editor theme"
  tabindex="-1"
  class="grid grid-cols-[repeat(auto-fill,minmax(10.5rem,1fr))] gap-3"
  {onkeydown}
>
  {#each EDITOR_THEME_IDS as id (id)}
    {@const c = editorPreviewColors(id)}
    {@const selected = value === id}
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      tabindex={selected ? 0 : -1}
      data-theme-id={id}
      class={cn(
        'flex min-w-0 flex-col overflow-hidden rounded-lg border text-left outline-none transition-[border-color,box-shadow] duration-150',
        'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring',
        selected ? 'border-primary/60 ring-1 ring-primary/25' : 'border-border/60 hover:border-border',
      )}
      onclick={() => onpick(id)}
    >
      <span
        class="flex gap-2 px-2.5 py-2 font-mono text-ui-3xs leading-relaxed"
        style="background:{c.bg};color:{c.fg};font-family:var(--editor-font-family, var(--font-mono))"
        aria-hidden="true"
      >
        <span class="flex shrink-0 flex-col text-right tabular-nums" style="color:{c.gutter}">
          {#each SAMPLE as _, i (i)}<span>{i + 1}</span>{/each}
        </span>
        <span class="flex min-w-0 flex-col overflow-hidden whitespace-pre">
          {#each SAMPLE as line, i (i)}
            <span class={cn('truncate', line[0][1] === 'comment' && 'italic')}>{#each line as [text, part], j (j)}<span style="color:{c[part]}">{text}</span>{/each}</span>
          {/each}
        </span>
      </span>
      <span
        class={cn(
          'flex h-7 items-center gap-1.5 border-t border-border/40 px-2.5 text-ui-xs',
          selected ? 'bg-primary/10 font-medium text-foreground' : 'text-foreground/85',
        )}
      >
        <span class="min-w-0 flex-1 truncate">{EDITOR_THEMES[id].label}</span>
        {#if selected}<Check class="size-3.5 shrink-0 text-primary" />{/if}
      </span>
    </button>
  {/each}
</div>
