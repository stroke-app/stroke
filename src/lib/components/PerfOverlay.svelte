<script>
  // Dev-only frame-rate HUD. Mounted from App.svelte behind `import.meta.env.DEV`,
  // so it never reaches a release build.
  //
  // It reads the rAF cadence, which is the rate the compositor is actually
  // painting this webview at - the same number a resize drag or a grid scroll
  // drops. Idle frames still tick at vsync, so a quiet app reads ~60: the number
  // only means something while something is moving.
  import { onMount } from 'svelte'
  import { cn } from '$lib/utils.js'

  const KEY = 'stroke:fps-overlay'

  let visible = $state(true)
  let fps = $state(0)
  /** Mean frame interval over the sample window, ms. */
  let frameMs = $state(0)
  /** Longest single frame in the window - where a stall shows up that an average hides. */
  let worstMs = $state(0)
  let renderer = $state('')

  /** Which GPU the webview ended up on. `llvmpipe` here means software rasterisation. */
  function readRenderer() {
    try {
      const canvas = document.createElement('canvas')
      const gl = /** @type {WebGLRenderingContext | null} */ (
        canvas.getContext('webgl2') || canvas.getContext('webgl')
      )
      if (!gl) return 'no webgl'
      const ext = gl.getExtension('WEBGL_debug_renderer_info')
      const name = ext
        ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)
        : gl.getParameter(gl.RENDERER)
      const label = String(name || 'unknown')
      // WebKit masks the renderer as "Apple GPU" on every platform (fingerprinting
      // protection), which on Linux or Windows names a GPU that is not there and
      // hides the one thing this line is for: whether it is llvmpipe.
      if (label === 'Apple GPU' && !/mac/i.test(navigator.platform)) return 'GPU name hidden by WebKit'
      return label
    } catch {
      return 'unknown'
    }
  }

  onMount(() => {
    try { visible = localStorage.getItem(KEY) !== '0' } catch {}
    renderer = readRenderer()

    /** @param {KeyboardEvent} e */
    function onKey(e) {
      if (e.key !== 'F8') return
      e.preventDefault()
      visible = !visible
      try { localStorage.setItem(KEY, visible ? '1' : '0') } catch {}
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  // The loop only runs while the HUD is on screen: an always-scheduled rAF keeps
  // the compositor awake for a number nobody is reading.
  $effect(() => {
    if (!visible) return
    let raf = 0
    let last = performance.now()
    let frames = 0
    let elapsed = 0
    let worst = 0
    /** @param {number} t */
    function loop(t) {
      const dt = t - last
      last = t
      frames++
      elapsed += dt
      if (dt > worst) worst = dt
      // Sample over 500ms rather than per frame, or the digits are unreadable.
      if (elapsed >= 500) {
        fps = Math.round((frames * 1000) / elapsed)
        frameMs = elapsed / frames
        worstMs = worst
        frames = 0
        elapsed = 0
        worst = 0
      }
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    return () => cancelAnimationFrame(raf)
  })

  // Deliberately not themed. A debug HUD that restyles itself per theme is one
  // more thing to misread at a glance, and this has to stay legible on top of
  // whatever it is measuring.
  const tone = $derived(fps >= 50 ? '#4ade80' : fps >= 30 ? '#fbbf24' : '#f87171')
</script>

{#if visible}
  <div
    class={cn(
      'pointer-events-none fixed top-1.5 left-1/2 z-[9999] -translate-x-1/2 select-none',
      'rounded border border-white/15 bg-black/85 px-2 py-1 text-center font-mono text-ui-2xs leading-tight',
    )}
    style:color={tone}
    aria-hidden="true"
  >
    <div>
      {fps} fps · {frameMs.toFixed(1)} ms · worst {worstMs.toFixed(0)} ms
    </div>
    <div class="text-white/45">{renderer} · F8 to hide</div>
  </div>
{/if}
