<script>
  // A row of colour swatches from the shared palette (stores/workspaces.js).
  // `allowNone` adds a "no colour" swatch first (connections may go uncoloured;
  // workspaces always have one).
  import { COLORS, isCustomColor } from '$lib/stores/workspaces.js'
  import { cn } from '$lib/utils.js'
  import Check from '@lucide/svelte/icons/check'
  import Ban from '@lucide/svelte/icons/ban'
  import Plus from '@lucide/svelte/icons/plus'

  /** @type {{ value?: string | null, onchange: (color: string | null) => void, allowNone?: boolean, label?: string }} */
  let { value = null, onchange, allowNone = false, label = 'Colour' } = $props()

  const custom = $derived(isCustomColor(value) ? value : null)
  // The native picker reports every drag step; only the last one needs to land.
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let pickTimer
  /** @param {Event} e */
  function pick(e) {
    const hex = /** @type {HTMLInputElement} */ (e.currentTarget).value.toLowerCase()
    clearTimeout(pickTimer)
    pickTimer = setTimeout(() => onchange(hex), 60)
  }
</script>

<div class="flex flex-wrap items-center gap-1.5" role="radiogroup" aria-label={label}>
  {#if allowNone}
    <button
      type="button"
      role="radio"
      aria-checked={!value}
      aria-label="No colour"
      title="No colour"
      class={cn('flex size-5 items-center justify-center rounded-full text-muted-foreground ring-offset-2 ring-offset-popover transition-[box-shadow,scale] duration-[120ms] ease-[var(--ease-out)] hover:text-foreground active:scale-[0.94]', !value && 'ring-2 ring-ring/60')}
      onclick={() => onchange(null)}
    >
      <Ban class="size-3 shrink-0" />
    </button>
  {/if}
  {#each COLORS as c (c.id)}
    <button
      type="button"
      role="radio"
      aria-checked={value === c.id}
      aria-label={c.label}
      title={c.label}
      class={cn('flex size-5 items-center justify-center rounded-full ring-offset-2 ring-offset-popover transition-[box-shadow,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.94]', c.dot, value === c.id && 'ring-2 ring-ring/60')}
      onclick={() => onchange(c.id)}
    >
      {#if value === c.id}<Check class="size-3 shrink-0 text-white" strokeWidth={3} />{/if}
    </button>
  {/each}
  <!-- Custom colour: a rainbow ring until one is picked, then that colour.
       The native picker sits inside the label, so a click opens it. -->
  <label
    title={custom ? `Custom ${custom}` : 'Custom colour'}
    class={cn(
      'relative flex size-5 cursor-pointer items-center justify-center rounded-full ring-offset-2 ring-offset-popover transition-shadow focus-within:ring-2 focus-within:ring-ring',
      custom && 'ring-2 ring-ring/60',
    )}
    style={custom
      ? `background: ${custom}`
      : 'background: conic-gradient(from 0deg, oklch(0.62 0.19 25), oklch(0.62 0.15 90), oklch(0.62 0.15 160), oklch(0.62 0.15 230), oklch(0.62 0.2 300), oklch(0.62 0.19 25))'}
  >
    <input
      type="color"
      role="radio"
      aria-checked={!!custom}
      aria-label="Custom colour"
      class="absolute inset-0 size-full cursor-pointer opacity-0"
      value={custom ?? '#6366f1'}
      oninput={pick}
      onchange={pick}
    />
    {#if custom}
      <Check class="pointer-events-none size-3 shrink-0 text-white" strokeWidth={3} />
    {:else}
      <span class="pointer-events-none flex size-3.5 items-center justify-center rounded-full bg-popover">
        <Plus class="size-2.5 shrink-0 text-foreground" strokeWidth={2.5} />
      </span>
    {/if}
  </label>
</div>
