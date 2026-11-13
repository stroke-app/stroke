<script>
  /**
   * The insert row's field for a column with a known set of values - an enum, a
   * boolean, anything the catalog can enumerate.
   *
   * It is an input first and a list second. A plain `<select>` makes you reach
   * for the mouse and hunt a list that can be sixty labels long, and it can't
   * express the one thing that matters most on an insert: leaving the field
   * alone so the database's own default applies. So: type to filter, arrows to
   * move, Enter to take the highlighted row - and the empty value is a real,
   * first-class row rather than a blank entry you have to guess at.
   */
  import { Popover, PopoverTrigger, PopoverContent } from '$lib/components/ui/popover/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** Current draft text. Empty string means "leave this column out". */
    value = '',
    /** @type {string[]} */
    options = [],
    /** What leaving the field empty does - shown as the first row's caption. */
    emptyLabel = 'NULL',
    /** Placeholder for the input when empty. */
    placeholder = '',
    /** Extra placeholder styling - required blanks read louder than optional ones. */
    placeholderClass = 'placeholder:text-muted-foreground',
    disabled = false,
    /** Grid wiring: identifies the field for focus restoration. */
    colName = '',
    onchange = /** @type {(next: string) => void} */ (() => {}),
    onfocus = /** @type {() => void} */ (() => {}),
  } = $props()

  let open = $state(false)
  let query = $state('')
  let active = $state(0)
  /** @type {HTMLInputElement | null} */
  let inputEl = $state(null)

  /**
   * Typing filters; not typing shows everything. `query` is only the *typed*
   * text - a value chosen from the list doesn't then filter the list down to
   * itself, which would hide every alternative the moment you picked one.
   */
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase()
    if (!q) return options
    return options.filter((o) => o.toLowerCase().includes(q))
  })

  /** The empty row plus the matches; index 0 is always "leave empty". */
  const rows = $derived([{ value: '', label: emptyLabel, empty: true }, ...filtered.map((o) => ({ value: o, label: o, empty: false }))])

  $effect(() => {
    if (!open) { query = ''; return }
    // Open with the current value highlighted, so Enter re-picks it rather than
    // silently clearing the field.
    const i = rows.findIndex((r) => r.value === value)
    active = i >= 0 ? i : 0
  })

  /** @param {string} next */
  function commit(next) {
    onchange(next)
    query = ''
    open = false
    inputEl?.focus()
  }

  /** @param {KeyboardEvent} e */
  function onKeydown(e) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      e.stopPropagation()
      if (!open) { open = true; return }
      const dir = e.key === 'ArrowDown' ? 1 : -1
      active = (active + dir + rows.length) % rows.length
      return
    }
    if (e.key === 'Enter' && open) {
      // Enter inside an open list takes the highlighted row. The grid's own
      // ⌘↵ submit is a different chord, so it still reaches the row handler.
      e.preventDefault()
      e.stopPropagation()
      commit(rows[active]?.value ?? '')
      return
    }
    if (e.key === 'Escape' && open) {
      e.preventDefault()
      e.stopPropagation()
      open = false
    }
  }

  /** @param {Event & { currentTarget: HTMLInputElement }} e */
  function onInput(e) {
    const text = e.currentTarget.value
    query = text
    active = 0
    open = true
    // Free text is kept as the value: an enum you typed correctly should not
    // require a trip through the list, and a mistyped one gets a precise error
    // from the insert validator rather than being silently dropped.
    onchange(text)
  }
</script>

<Popover bind:open>
  <div class="relative flex w-full min-w-0 items-center">
    <!-- The trigger is the anchor, not the control: it spans the field so the
         list lines up with it, and takes no pointer events so the input keeps
         every click. Opening is the chevron's job (and the arrow keys'). -->
    <PopoverTrigger
      tabindex={-1}
      aria-hidden="true"
      class="pointer-events-none absolute inset-0 -z-10"
    />
    <input
      bind:this={inputEl}
      data-new-row-input={colName}
      type="text"
      {disabled}
      {placeholder}
      value={query || value}
      autocomplete="off"
      spellcheck="false"
      class={cn(
        'w-full min-w-0 bg-transparent pr-5 font-mono text-ui-sm text-foreground outline-none disabled:opacity-50',
        placeholderClass,
      )}
      oninput={onInput}
      onkeydown={onKeydown}
      onfocus={onfocus}
    />
    <button
      type="button"
      {disabled}
      tabindex={-1}
      aria-label="Show values"
      class="absolute right-0 inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
      onclick={() => { open = !open; inputEl?.focus() }}
    >
      <Icon name="chevron-down" class={cn('size-3 transition-transform', open && 'rotate-180')} />
    </button>
  </div>

  <!-- Tight rows, one accent, no ornament: the list is a set of values, and at
       this size a checkmark column costs more than the state it communicates
       (the highlighted row already carries it). -->
  <PopoverContent
    align="start"
    sideOffset={4}
    trapFocus={false}
    onOpenAutoFocus={(e) => e.preventDefault()}
    class="max-h-56 w-[var(--bits-popover-anchor-width)] min-w-40 overflow-y-auto p-1"
  >
    {#if rows.length === 1 && query}
      <p class="px-2 py-1.5 font-mono text-ui-xs text-muted-foreground">No value matches “{query}”</p>
    {/if}
    {#each rows as row, i (row.value + String(i))}
      <button
        type="button"
        onmouseenter={() => (active = i)}
        onclick={() => commit(row.value)}
        class={cn(
          'flex h-7 w-full items-center gap-2 rounded-md px-2 text-left font-mono text-ui-xs transition-colors',
          row.empty ? 'text-muted-foreground' : 'text-foreground',
          i === active ? 'bg-accent text-accent-foreground' : '',
        )}
      >
        <span class="min-w-0 flex-1 truncate">{row.label}</span>
        {#if row.value === value}
          <span class="shrink-0 text-ui-3xs text-muted-foreground">current</span>
        {/if}
      </button>
    {/each}
  </PopoverContent>
</Popover>
