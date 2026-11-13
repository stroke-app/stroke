<script>
  import { Button } from '$lib/components/ui/button/index.js'
  // Dedicated editor for SQL array cells (Postgres text[]/int[]/… , ClickHouse
  // Array(...)). Prisma-Studio-style add / remove / reorder / insert, instead of
  // treating the array as raw JSON. Emits the edited JS array back to the parent,
  // which serializes it to a Postgres array literal for the write.
  import { tick } from 'svelte'
  import Plus from '@lucide/svelte/icons/plus'
  import X from '@lucide/svelte/icons/x'
  import GripVertical from '@lucide/svelte/icons/grip-vertical'
  import CircleSlash from '@lucide/svelte/icons/circle-slash'
  import Brackets from '@lucide/svelte/icons/brackets'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    /** Column name shown in the header. */
    column = '',
    /** Element type label, e.g. "text", "int4" (from the column type minus []). */
    elementType = '',
    /** Initial elements (array of primitives / null). */
    value = /** @type {any[]} */ ([]),
    /** Called with the new array on save. */
    onsave = /** @type {(next: any[]) => void} */ (() => {}),
  } = $props()

  // Local working copy - each row is { v: string|null }. Strings map to array
  // elements; `null` is a genuine SQL NULL element.
  let items = $state(/** @type {{ v: string | null }[]} */ ([]))
  /** @type {HTMLInputElement[]} */
  let inputEls = []

  // Seed the working copy whenever the editor opens for a new cell.
  $effect(() => {
    if (open) {
      // Built into a local and read from it. Assigning `items` and then reading
      // `items.length` on the next line made this effect depend on the state it
      // had just written - and since each run assigns a brand new array, the
      // dependency fired again every time. Opening an array cell took the whole
      // view down with effect_update_depth_exceeded.
      const next = (Array.isArray(value) ? value : []).map((el) => ({
        v: el === null || el === undefined ? null : String(el),
      }))
      items = next
      inputEls.length = next.length
    }
  })

  async function addItem() {
    items = [...items, { v: '' }]
    await tick()
    inputEls[items.length - 1]?.focus()
  }
  function removeAt(i) {
    items = items.filter((_, j) => j !== i)
    // Index-keyed binds shift down; drop the now-stale ref at the old tail so a
    // later addItem() focuses the right (freshly rebound) input.
    inputEls.length = items.length
  }

  // ── Drag to reorder ──────────────────────────────────────────────────────────
  let dragIndex = $state(/** @type {number | null} */ (null))
  let overIndex = $state(/** @type {number | null} */ (null))
  function onDragStart(i, e) {
    dragIndex = i
    if (e.dataTransfer) { e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', String(i)) }
  }
  function onDragOver(i, e) {
    e.preventDefault()
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
    overIndex = i
  }
  function onDrop(i) {
    if (dragIndex === null || dragIndex === i) { dragIndex = null; overIndex = null; return }
    const next = items.slice()
    const [moved] = next.splice(dragIndex, 1)
    next.splice(i, 0, moved)
    items = next
    dragIndex = null
    overIndex = null
  }
  function onDragEnd() { dragIndex = null; overIndex = null }
  function toggleNull(i) {
    items = items.map((it, j) => (j === i ? { v: it.v === null ? '' : null } : it))
  }
  function clearAll() {
    items = []
    inputEls.length = 0
  }

  function save() {
    onsave(items.map((it) => (it.v === null ? null : it.v)))
    open = false
  }
  function cancel() {
    open = false
  }
</script>

{#if open}
  <!-- Centered modal overlay; Escape/backdrop cancels. -->
  <div
    class="fixed inset-0 z-[80] flex items-center justify-center bg-black/65 p-6 data-open:animate-in data-open:fade-in-0"
    data-open
    role="button"
    tabindex="-1"
    onclick={(e) => { if (e.target === e.currentTarget) cancel() }}
    onkeydown={(e) => { if (e.key === 'Escape') { e.preventDefault(); cancel() } }}
  >
    <div class="flex max-h-[80vh] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-border/50 bg-background elevate-3-rim">
      <!-- Header -->
      <div class="flex items-center gap-3 border-b border-border/15 px-4 py-3.5">
        <div class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
          <Brackets class="size-4 shrink-0" />
        </div>
        <div class="min-w-0 flex-1">
          <p class="truncate text-ui-sm font-semibold tracking-tight text-foreground antialiased">{column}</p>
          <p class="truncate text-ui-2xs leading-relaxed text-muted-foreground">
            <span class="font-mono">{elementType || 'array'}[]</span>
            <span class="text-muted-foreground">·</span>
            <span class="tabular-nums">{items.length}</span> {items.length === 1 ? 'element' : 'elements'}
          </p>
        </div>
        <button
          type="button"
          aria-label="Close"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,transform] duration-150 ease-out hover:bg-accent hover:text-foreground active:scale-[0.94]"
          onclick={cancel}
        >
          <X class="size-4 shrink-0" />
        </button>
      </div>

      <!-- Elements. One bordered surface with hairline separators rather than a
           bordered box per row: eleven outlines stacked up read as eleven
           fields to fill in, when what this is is one list. -->
      <div class="min-h-0 flex-1 overflow-y-auto px-3 py-2.5">
        {#if items.length === 0}
          <div class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border/50 px-4 py-8 text-center">
            <p class="text-ui-xs text-muted-foreground">Empty array <span class="font-mono">{'{}'}</span></p>
            <button
              type="button"
              class="inline-flex items-center gap-1.5 rounded-md border border-border/50 px-2.5 py-1 text-ui-2xs font-medium text-muted-foreground transition-[color,background-color,transform] duration-150 ease-out hover:bg-muted/40 hover:text-foreground active:scale-[0.96]"
              onclick={addItem}
            >
              <Plus class="size-3.5 shrink-0" /> Add element
            </button>
          </div>
        {:else}
          <div class="flex flex-col overflow-hidden rounded-lg border border-border/40 bg-muted/10">
            {#each items as item, i (i)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class={cn(
                  'group relative flex h-8 items-center gap-1.5 px-1.5 transition-[opacity,background-color] duration-150',
                  i > 0 && 'border-t border-border/25',
                  dragIndex === i && 'opacity-40',
                  overIndex === i && dragIndex !== i ? 'bg-primary/10' : 'hover:bg-muted/25',
                )}
                ondragover={(e) => onDragOver(i, e)}
                ondrop={() => onDrop(i)}
              >
                <!-- Drop indicator line above the hovered row -->
                {#if overIndex === i && dragIndex !== null && dragIndex !== i}
                  <span class="pointer-events-none absolute inset-x-0 -top-px h-0.5 bg-primary"></span>
                {/if}
                <!-- Drag handle -->
                <button
                  type="button"
                  aria-label="Drag to reorder"
                  draggable="true"
                  ondragstart={(e) => onDragStart(i, e)}
                  ondragend={onDragEnd}
                  class="hit-area flex size-5 shrink-0 cursor-grab items-center justify-center rounded-md text-muted-foreground/35 transition-colors group-hover:text-muted-foreground active:cursor-grabbing"
                >
                  <GripVertical class="size-3.5 shrink-0" />
                </button>
                <span class="w-6 shrink-0 pr-1 text-right font-mono text-ui-3xs tabular-nums text-muted-foreground/60">{i}</span>
                {#if item.v === null}
                  <button
                    type="button"
                    class="flex h-7 flex-1 items-center rounded-md px-2 text-left font-mono text-ui-xs italic text-warning transition-colors hover:bg-muted/40 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring"
                    onclick={() => toggleNull(i)}
                    title="Click to enter a value"
                  >NULL</button>
                {:else}
                  <input
                    bind:this={inputEls[i]}
                    bind:value={item.v}
                    placeholder="value"
                    spellcheck="false"
                    autocomplete="off"
                    class="h-7 min-w-0 flex-1 rounded-md bg-transparent px-2 font-mono text-ui-xs text-foreground outline-none transition-colors placeholder:text-muted-foreground/50 hover:bg-muted/25 focus:bg-muted/30 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring"
                    onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addItem() } }}
                  />
                {/if}
                <!-- Always there, just quiet. Fading them in on hover moved
                     nothing but made the row look different every time the
                     pointer crossed it, and left them unreachable by keyboard. -->
                <div class="flex shrink-0 items-center gap-0.5">
                  <button type="button" aria-label={item.v === null ? 'Clear NULL' : 'Set NULL'}
                    class={cn(
                      'inline-flex size-6 items-center justify-center rounded-md transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring',
                      item.v === null ? 'text-warning' : 'text-muted-foreground/40 group-hover:text-muted-foreground hover:text-foreground',
                    )}
                    title={item.v === null ? 'Clear NULL' : 'Set NULL'}
                    onclick={() => toggleNull(i)}><CircleSlash class="size-3.5 shrink-0" /></button>
                  <button type="button" aria-label="Remove element"
                    class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground/40 transition-colors group-hover:text-muted-foreground hover:bg-destructive/15 hover:text-destructive focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-ring"
                    title="Remove"
                    onclick={() => removeAt(i)}><X class="size-3.5 shrink-0" /></button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="flex items-center gap-1.5 border-t border-border/15 bg-muted/10 px-3 py-2.5">
        <button
          type="button"
          class= "field-surface inline-flex h-8 items-center gap-1.5 px-2.5 text-ui-xs font-medium text-muted-foreground transition-[color,background-color,transform] duration-150 ease-out hover:bg-muted/40 hover:text-foreground active:scale-[0.96]"
          onclick={addItem}
        >
          <Plus class="size-3.5 shrink-0" /> Add
        </button>
        {#if items.length > 0}
          <button
            type="button"
            class="inline-flex h-8 items-center rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
            onclick={clearAll}
          >Clear all</button>
        {/if}
        <div class="ml-auto flex items-center gap-2">
          <button
            type="button"
            class= "field-surface inline-flex h-8 items-center px-3 text-ui-xs text-muted-foreground transition-[color,background-color,transform] duration-150 ease-out hover:bg-muted/40 hover:text-foreground active:scale-[0.96]"
            onclick={cancel}
          >Cancel</button>
          <!-- bg-primary, the way ConfirmDialog and the update dialog do it. It
               was the only primary button in the app painted in the foreground
               colour. -->
          <Button
            
            onclick={save}
>Save</Button>
        </div>
      </div>
    </div>
  </div>
{/if}
