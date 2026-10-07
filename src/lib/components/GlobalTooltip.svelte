<!--
  GlobalTooltip, one styled tooltip for the whole app.

  Delegates off `title` (and opt-in `data-tip`) attributes so every element that
  used to fall back to the browser's native tooltip now renders the same styled
  bubble: consistent arrow, distance, delay and hover-persistence everywhere.

  Mounted once at the app root (App.svelte). Individual call sites keep using
  plain `title="…"`; no per-element wiring needed.

  Opt out on any element with `data-no-tip`.
-->
<script>
  import { tick } from 'svelte'
  import { cn } from '$lib/utils.js'

  const DELAY = 450 // ms before showing (a touch slower so it doesn't flash on pass-through)
  const GAP = 8 // distance between trigger and tooltip (arrow lives inside this)
  const EDGE = 8 // min gap from the viewport edge
  const ARROW = 5 // half the arrow's diagonal footprint

  let text = $state('')
  let shown = $state(false)
  let side = $state(/** @type {'top' | 'bottom'} */ ('bottom'))
  let top = $state(0)
  let left = $state(0)
  let arrowLeft = $state(0)

  /** @type {HTMLDivElement | null} */
  let tipEl = null
  /** @type {Element | null} */
  let currentEl = null
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let showTimer

  /** Pull the tooltip text off an element, suppressing the native `title`. */
  function readTip(/** @type {Element} */ el) {
    const dataTip = el.getAttribute('data-tip')
    if (dataTip != null) return dataTip.trim()
    const title = el.getAttribute('title')
    if (title != null) {
      // Stash + strip so the OS tooltip never double-fires while we own it.
      el.setAttribute('data-stroke-title', title)
      el.removeAttribute('title')
      return title.trim()
    }
    return el.getAttribute('data-stroke-title')?.trim() ?? ''
  }

  /**
   * Elements with a tooltip, the stashed `title` included: the pointer moving
   * from a button onto its own icon fires pointerover again, and with only
   * `[title]` that found nothing (the title was stripped), read it as leaving,
   * put the title back, and the native tooltip showed instead of this one.
   */
  const TIP_SELECTOR = '[title], [data-tip], [data-stroke-title]'

  /** Give an element its native `title` back once we're done with it. */
  function restoreTitle(/** @type {Element | null} */ el) {
    if (!el) return
    const stashed = el.getAttribute('data-stroke-title')
    if (stashed != null) {
      el.setAttribute('title', stashed)
      el.removeAttribute('data-stroke-title')
    }
  }

  function hide() {
    clearTimeout(showTimer)
    restoreTitle(currentEl)
    currentEl = null
    shown = false
  }

  async function show(/** @type {Element} */ el, /** @type {string} */ value) {
    text = value
    await tick()
    if (currentEl !== el || !tipEl) return // moved on before we could measure

    const r = el.getBoundingClientRect()
    const tw = tipEl.offsetWidth
    const th = tipEl.offsetHeight
    const vw = window.innerWidth
    const vh = window.innerHeight

    // Prefer below; flip above when it would clip the bottom edge.
    if (r.bottom + GAP + th > vh - EDGE && r.top - GAP - th > EDGE) {
      side = 'top'
      top = r.top - GAP - th
    } else {
      side = 'bottom'
      top = r.bottom + GAP
    }

    const anchorX = r.left + r.width / 2
    left = Math.max(EDGE, Math.min(anchorX - tw / 2, vw - tw - EDGE))
    arrowLeft = Math.max(ARROW + 4, Math.min(anchorX - left, tw - ARROW - 4))

    shown = true
  }

  function onPointerOver(/** @type {PointerEvent} */ e) {
    const target = /** @type {Element | null} */ (e.target)
    const el = target?.closest?.(TIP_SELECTOR)
    if (el === currentEl) return
    if (currentEl) hide()
    if (!el || el.hasAttribute('data-no-tip')) return

    const value = readTip(el)
    if (!value) return

    currentEl = el
    clearTimeout(showTimer)
    showTimer = setTimeout(() => {
      if (currentEl === el) show(el, value)
    }, DELAY)
  }

  function onPointerOut(/** @type {PointerEvent} */ e) {
    if (!currentEl) return
    const to = /** @type {Node | null} */ (e.relatedTarget)
    // Still inside the current trigger - keep it up.
    if (to && currentEl.contains(to)) return
    hide()
  }

  $effect(() => {
    // passive: hide() never preventDefaults, so scrolling must not wait on us.
    const opts = { capture: true, passive: true }
    document.addEventListener('pointerover', onPointerOver, true)
    document.addEventListener('pointerout', onPointerOut, true)
    document.addEventListener('pointerdown', hide, true)
    document.addEventListener('wheel', hide, opts)
    document.addEventListener('scroll', hide, opts)
    window.addEventListener('blur', hide)
    return () => {
      clearTimeout(showTimer)
      restoreTitle(currentEl)
      document.removeEventListener('pointerover', onPointerOver, true)
      document.removeEventListener('pointerout', onPointerOut, true)
      document.removeEventListener('pointerdown', hide, true)
      document.removeEventListener('wheel', hide, opts)
      document.removeEventListener('scroll', hide, opts)
      window.removeEventListener('blur', hide)
    }
  })
</script>

<div
  bind:this={tipEl}
  role="tooltip"
  aria-hidden={!shown}
  class={cn(
    'pointer-events-none fixed z-[300] max-w-xs rounded-lg border border-border/60 bg-popover px-2.5 py-1.5',
    'text-ui-xs leading-snug text-popover-foreground shadow-md',
    'origin-[var(--tt-origin)] transition-[opacity,transform] duration-[130ms] ease-out',
    'motion-reduce:transition-none',
    shown ? 'scale-100 opacity-100' : 'scale-[0.96] opacity-0',
  )}
  style="top:{top}px; left:{left}px; --tt-origin:{side === 'top' ? 'bottom' : 'top'};"
  style:visibility={text ? 'visible' : 'hidden'}
>
  <span class="whitespace-pre-line break-words">{text}</span>
  <!-- Arrow: a rotated square with two visible borders, tucked against the edge. -->
  <span
    class={cn(
      'absolute size-2 rotate-45 border-border/60 bg-popover',
      side === 'bottom' ? '-top-1 border-l border-t' : '-bottom-1 border-b border-r',
    )}
    style="left:{arrowLeft}px; margin-left:-4px;"
  ></span>
</div>
