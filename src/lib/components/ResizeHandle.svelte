<script>
  import { cn } from '$lib/utils.js'

  let {
    /** @type {'x' | 'y'} */
    axis = 'x',
    /** @type {'start' | 'end'} */
    edge = 'end',
    onresizestart = () => {},
    onresize = () => {},
    onresizeend = () => {},
  } = $props()

  /** @param {PointerEvent} e */
  function handlePointerDown(e) {
    if (e.button !== 0) return
    e.preventDefault()
    onresizestart()

    const bodyClass = axis === 'x' ? 'is-resizing-x' : 'is-resizing-y'
    document.body.classList.add(bodyClass)

    const el = /** @type {HTMLElement} */ (e.currentTarget)
    el.setPointerCapture(e.pointerId)
    const startX = e.clientX
    const startY = e.clientY

    // Coalesce to one `onresize` per animation frame.
    //
    // A pointermove stream runs at the pointer's rate, not the display's - 120Hz+
    // on a trackpad or a high-polling mouse - and every call here writes a panel
    // width, which reflows the whole shell. Where the panel sits next to the data
    // grid that also means the grid's ResizeObserver fires, its canvas backing
    // store is reallocated (which clears it) and the frame is repainted from
    // scratch. Running that several times per PAINTED frame is pure waste, and it
    // is what made dragging a panel next to the grid - the inspector, the related-
    // rows dock, the log panel - judder and flash.
    //
    // rAF also puts the write in the same frame the browser is about to paint, so
    // the handle tracks the pointer exactly as closely as before.
    let rafId = 0
    let pending = 0
    let hasPending = false

    const flush = () => {
      rafId = 0
      if (!hasPending) return
      hasPending = false
      onresize(pending)
    }

    /** @param {PointerEvent} ev */
    function onMove(ev) {
      const delta = axis === 'x' ? ev.clientX - startX : ev.clientY - startY
      pending = edge === 'end' ? delta : -delta
      hasPending = true
      if (!rafId) rafId = requestAnimationFrame(flush)
    }

    /** @param {PointerEvent} ev */
    function onUp(ev) {
      document.body.classList.remove(bodyClass)
      el.releasePointerCapture(ev.pointerId)
      el.removeEventListener('pointermove', onMove)
      el.removeEventListener('pointerup', onUp)
      el.removeEventListener('pointercancel', onUp)
      // Land on the last position the pointer actually reached, rather than on
      // whichever frame happened to win the race with pointerup.
      if (rafId) cancelAnimationFrame(rafId)
      flush()
      onresizeend()
    }

    el.addEventListener('pointermove', onMove)
    el.addEventListener('pointerup', onUp)
    el.addEventListener('pointercancel', onUp)
  }

  const STEP = 20

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    let delta = 0
    if (axis === 'x') {
      if (e.key === 'ArrowRight') delta = STEP
      else if (e.key === 'ArrowLeft') delta = STEP * -1
    } else {
      if (e.key === 'ArrowDown') delta = STEP
      else if (e.key === 'ArrowUp') delta = STEP * -1
    }
    if (!delta) return
    e.preventDefault()
    onresizestart()
    onresize(edge === 'end' ? delta : -delta)
    onresizeend()
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  role="separator"
  aria-orientation={axis === 'x' ? 'vertical' : 'horizontal'}
  aria-label="Resize panel"
  tabindex="0"
  class={cn(
    'studio-resize-handle group relative z-20 shrink-0 touch-none select-none focus-visible:outline-none',
    axis === 'x' ? 'h-full w-1 cursor-col-resize' : 'h-1 w-full cursor-row-resize',
  )}
  onpointerdown={handlePointerDown}
  onkeydown={handleKeydown}
>
  <!-- Visual-only line, pointer-events-none so it never shadows the cursor -->
  <div
    class={cn(
      // A full-length hairline exactly on the panel's edge, so it meets the
      // title bar's edge line above it. It used to be rounded, translucent,
      // inset 8px at each end and centred 2px inside the grab area: a short,
      // soft line that never lined up with anything.
      'pointer-events-none absolute bg-border/50 group-hover:bg-primary/70 group-active:bg-primary group-focus-visible:bg-primary/70',
      axis === 'x' && 'inset-y-0 w-px',
      axis === 'x' && (edge === 'start' ? 'left-0' : 'right-0'),
      axis === 'y' && 'inset-x-0 h-px',
      axis === 'y' && (edge === 'start' ? 'top-0' : 'bottom-0'),
    )}
  ></div>
</div>
