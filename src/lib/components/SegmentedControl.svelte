<script>
  /**
   * Compact segmented control. Single mode is a radio group over `value`;
   * `multiple` turns the segments into toggles over a `value` array.
   */
  import { cn } from '$lib/utils.js'

  let {
    /** @type {Array<{ value: string, label: string, disabled?: boolean }>} */
    options = [],
    /** @type {any} */
    value = $bindable(),
    multiple = false,
    disabled = false,
    mono = false,
    ariaLabel = '',
    id = undefined,
    class: className = '',
    /** @type {(v: any) => void} */
    onchange = () => {},
  } = $props()

  /** @param {string} v */
  const isOn = (v) => (multiple ? Array.isArray(value) && value.includes(v) : value === v)

  /** @param {string} v */
  function pick(v) {
    if (multiple) {
      const cur = Array.isArray(value) ? value : []
      value = cur.includes(v) ? cur.filter((x) => x !== v) : [...cur, v]
    } else {
      value = v
    }
    onchange(value)
  }
</script>

<div
  {id}
  role={multiple ? 'group' : 'radiogroup'}
  aria-label={ariaLabel || undefined}
  class={cn('flex h-8 w-full items-stretch gap-0.5 rounded-md bg-muted/50 p-0.5', disabled && 'opacity-50', className)}
>
  {#each options as o (o.value)}
    {@const on = isOn(o.value)}
    <button
      type="button"
      role={multiple ? undefined : 'radio'}
      aria-checked={multiple ? undefined : on}
      aria-pressed={multiple ? on : undefined}
      disabled={disabled || o.disabled}
      class={cn(
        'min-w-0 flex-1 truncate rounded-sm px-2 text-ui-xs whitespace-nowrap transition-colors disabled:pointer-events-none',
        mono && 'font-mono',
        on
          ? 'bg-background font-medium text-foreground ring-1 ring-border/60'
          : 'text-muted-foreground hover:text-foreground',
        o.disabled && !on && 'opacity-50',
      )}
      onclick={() => pick(o.value)}
    >{o.label}</button>
  {/each}
</div>
