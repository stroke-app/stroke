<script>
  // Rows-per-page as an inline segmented control (footer of the NoSQL pages):
  // one click, no menu, and nothing to truncate.
  import { cn } from '$lib/utils.js'

  /** @type {{ value: number, sizes?: number[], onchange: (size: number) => void, label?: string }} */
  let { value, sizes = [20, 50, 100], onchange, label = 'Rows' } = $props()
</script>

<div class="flex items-center gap-1.5">
  <span class="text-ui-2xs text-muted-foreground" aria-hidden="true">{label}</span>
  <div class="flex h-6 items-center gap-0.5 rounded-[var(--radius-field)] bg-muted/40 p-0.5" role="radiogroup" aria-label={`${label} per page`}>
    {#each sizes as n (n)}
      <button
        type="button"
        role="radio"
        aria-checked={value === n}
        class={cn(
          'inline-flex h-5 min-w-7 items-center justify-center rounded-[calc(var(--radius-field)-2px)] px-1.5 font-mono text-ui-2xs tabular-nums transition-colors',
          value === n ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
        )}
        onclick={() => value !== n && onchange(n)}
      >{n}</button>
    {/each}
  </div>
</div>
