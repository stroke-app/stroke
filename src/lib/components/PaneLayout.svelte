<script>
  import Self from './PaneLayout.svelte'
  import { cn } from '$lib/utils.js'

  /** @typedef {import('$lib/pane-layout.js').PaneNode} PaneNode */
  /** @typedef {import('$lib/pane-layout.js').GroupNode} GroupNode */
  /** @typedef {import('$lib/pane-layout.js').DropEdge} DropEdge */

  let {
    /** @type {PaneNode} */
    node,
    /** @type {string | null} */
    focusedGroupId = null,
    /** True when >1 pane exists - enables the focused-pane accent (noise when single). */
    multiPane = false,
    /** Current pointer-drag drop target (group + edge), or null. Drives the hint. */
    dropTarget = /** @type {{ groupId: string, edge: DropEdge } | null} */ (null),
    /** @type {import('svelte').Snippet<[GroupNode, boolean]>} */
    renderGroup,
    /** @type {(splitId: string, sizes: [number, number]) => void} */
    onresize = () => {},
    /** @type {(groupId: string) => void} */
    onfocusgroup = () => {},
  } = $props()

  /** @type {HTMLElement | null} */
  let splitEl = $state(null)

  /** @param {PointerEvent} e */
  function startResize(e) {
    if (!splitEl || node.type !== 'split') return
    e.preventDefault()
    const rect = splitEl.getBoundingClientRect()
    const horizontal = node.dir === 'row'
    const splitId = node.id
    let raf = 0
    /** @type {number | null} */
    let pendingPct = null
    const flush = () => {
      raf = 0
      if (pendingPct != null) onresize(splitId, [pendingPct, 100 - pendingPct])
    }
    /** @param {PointerEvent} ev */
    const move = (ev) => {
      const total = horizontal ? rect.width : rect.height
      if (total <= 0) return
      const pos = horizontal ? ev.clientX - rect.left : ev.clientY - rect.top
      let pct = (pos / total) * 100
      pendingPct = Math.min(90, Math.max(10, pct))
      // Coalesce to one resize per animation frame so a heavy pane (the live
      // grid) can't stall the drag - the splitter stays glued to the pointer.
      if (!raf) raf = requestAnimationFrame(flush)
    }
    const up = () => {
      if (raf) cancelAnimationFrame(raf)
      raf = 0
      if (pendingPct != null) onresize(splitId, [pendingPct, 100 - pendingPct])
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      document.body.style.userSelect = ''
      document.body.style.cursor = ''
    }
    document.body.style.userSelect = 'none'
    document.body.style.cursor = horizontal ? 'col-resize' : 'row-resize'
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  /** Position/size of the drop-hint overlay for a given edge (translucent preview). */
  const HINT_CLASS = /** @type {Record<DropEdge, string>} */ ({
    left: 'left-0 top-0 h-full w-1/2',
    right: 'right-0 top-0 h-full w-1/2',
    top: 'left-0 top-0 w-full h-1/2',
    bottom: 'left-0 bottom-0 w-full h-1/2',
    center: 'inset-0',
  })
</script>

{#if node.type === 'split'}
  <div
    bind:this={splitEl}
    class={cn('flex min-h-0 min-w-0 flex-1', node.dir === 'row' ? 'flex-row' : 'flex-col')}
  >
    <div class="flex min-h-0 min-w-0 flex-col" style="flex: 0 0 {node.sizes[0]}%">
      <Self node={node.children[0]} {focusedGroupId} {multiPane} {dropTarget} {renderGroup} {onresize} {onfocusgroup} />
    </div>

    <!-- Splitter: a slim track with an always-visible hairline, a hover-revealed
         grip, and a widened invisible hit area for easy grabbing. -->
    <div
      class={cn(
        'group/split relative z-10 shrink-0 touch-none select-none',
        node.dir === 'row' ? 'w-1 cursor-col-resize' : 'h-1 cursor-row-resize',
      )}
      onpointerdown={startResize}
      role="separator"
      aria-orientation={node.dir === 'row' ? 'vertical' : 'horizontal'}
      tabindex="-1"
    >
      <!-- hairline (always visible), brightens on hover/drag -->
      <div
        class={cn(
          'absolute bg-border/60 transition-colors group-hover/split:bg-primary/70 group-active/split:bg-primary',
          node.dir === 'row' ? 'inset-y-0 left-1/2 w-px -translate-x-1/2' : 'inset-x-0 top-1/2 h-px -translate-y-1/2',
        )}
      ></div>
      <!-- grip bar, revealed on hover -->
      <div
        class={cn(
          'absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary/80 opacity-0 transition-opacity group-hover/split:opacity-100',
          node.dir === 'row' ? 'h-8 w-0.5' : 'h-0.5 w-8',
        )}
      ></div>
      <!-- widened invisible hit area, generous target so it's easy to grab -->
      <div
        class={cn(
          'absolute',
          node.dir === 'row' ? '-left-2 -right-2 inset-y-0' : '-top-2 -bottom-2 inset-x-0',
        )}
      ></div>
    </div>

    <div class="flex min-h-0 min-w-0 flex-col" style="flex: 0 0 {node.sizes[1]}%">
      <Self node={node.children[1]} {focusedGroupId} {multiPane} {dropTarget} {renderGroup} {onresize} {onfocusgroup} />
    </div>
  </div>
{:else}
  {@const isFocused = node.id === focusedGroupId}
  {@const hintEdge = dropTarget && dropTarget.groupId === node.id ? dropTarget.edge : null}
  <!-- data-pane-group lets the drag orchestrator hit-test which group + edge the
       pointer is over via elementFromPoint. -->
  <div
    class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
    data-pane-group={node.id}
    role="group"
    onpointerdowncapture={() => onfocusgroup(node.id)}
  >
    {@render renderGroup(node, isFocused)}

    <!-- Inactive split panes are intentionally NOT dimmed, every pane renders at
         full brightness so all views stay readable at once. -->

    <!-- Drop hint: translucent preview of where the dragged tab will land. -->
    {#if hintEdge}
      <div class="pointer-events-none absolute inset-0 z-40">
        <div
          class={cn(
            'absolute rounded-md bg-primary/20 ring-2 ring-inset ring-primary/70 transition-[inset,width,height] duration-75',
            HINT_CLASS[hintEdge],
          )}
        ></div>
      </div>
    {/if}
  </div>
{/if}
