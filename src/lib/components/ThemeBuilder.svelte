<script>
  // The theme builder: build the Custom theme from colours on a pad, the way
  // Arc does it. Everything previews live on the real app behind the dialog;
  // Apply keeps it, Cancel (or closing) puts the previous look back.
  //
  //   mode      - auto (follow macOS) / light / dark
  //   pad       - angle is hue, distance from the centre is strength; one to
  //               three handles (surfaces, accent, sidebar - custom-theme.js)
  //   swatches  - presets for the selected handle
  //   wave      - intensity: how much colour the surfaces carry
  //   dial      - grain: film-grain texture over the window
  import { untrack } from 'svelte'
  import { get } from 'svelte/store'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Sparkles from '@lucide/svelte/icons/sparkles'
  import Sun from '@lucide/svelte/icons/sun'
  import Moon from '@lucide/svelte/icons/moon'
  import Plus from '@lucide/svelte/icons/plus'
  import Minus from '@lucide/svelte/icons/minus'
  import Shuffle from '@lucide/svelte/icons/shuffle'
  import ChevronLeft from '@lucide/svelte/icons/chevron-left'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import { Button } from '$lib/components/ui/button/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import {
    customTheme, saveCustomTheme, revertCustomTheme, swatchCss, PRESETS, MAX_COLORS,
    encodeCustomTheme, decodeCustomTheme,
  } from '$lib/stores/custom-theme.js'
  import { previewTheme, endThemePreview, updateSettings } from '$lib/stores/settings.js'

  let { open = $bindable(false) } = $props()

  /** @type {import('$lib/stores/custom-theme.js').CustomTheme} */
  let draft = $state(structuredClone(get(customTheme)))
  let active = $state(0)
  let applied = false

  // A fresh copy each time it opens, so a cancelled session leaves nothing.
  // Untracked: the preview below writes the store, and re-reading it here
  // would reset the draft on every change.
  $effect(() => {
    if (!open) return
    untrack(() => {
      draft = structuredClone(get(customTheme))
      active = 0
      applied = false
    })
  })

  // Live preview: the app behind the dialog repaints, at most once a frame.
  // Painting per pointer event regenerated every token and re-applied the
  // whole theme 120 times a second on a trackpad - that was the lag.
  let previewFrame = 0
  $effect(() => {
    if (!open) return
    const snapshot = $state.snapshot(draft)
    cancelAnimationFrame(previewFrame)
    previewFrame = requestAnimationFrame(() => {
      customTheme.set(snapshot)
      previewTheme('custom')
    })
    return () => cancelAnimationFrame(previewFrame)
  })

  function close(/** @type {boolean} */ keep) {
    if (keep) {
      saveCustomTheme($state.snapshot(draft))
      updateSettings({ theme: 'custom' })
      applied = true
    } else {
      revertCustomTheme()
      endThemePreview()
    }
    open = false
  }

  /** Closing any other way than Apply puts everything back. @param {boolean} next */
  function onOpenChange(next) {
    if (next) return
    if (!applied) {
      revertCustomTheme()
      endThemePreview()
    }
  }

  // ── Share codes ────────────────────────────────────────────────────────────
  let importing = $state(false)
  let importText = $state('')
  async function copyCode() {
    try {
      await navigator.clipboard.writeText(encodeCustomTheme($state.snapshot(draft)))
      toast.success('Theme code copied', { description: 'Paste it into Import in anyone’s theme builder.' })
    } catch {
      toast.error('Could not copy the theme code')
    }
  }
  /** Load a pasted code into the draft (it previews; Apply keeps it). */
  function importCode(/** @type {string} */ text) {
    const t = decodeCustomTheme(text)
    if (!t) {
      if (text.trim()) toast.error('That isn’t a Stroke theme code', { description: 'Codes start with stroke-theme:' })
      return
    }
    draft = t
    active = 0
    importing = false
    importText = ''
  }

  // ── Pad ────────────────────────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let pad = $state(null)
  /** Handle position on the pad, as percentages. @param {{ h: number, s: number }} c */
  const at = (c) => {
    const r = (c.h * Math.PI) / 180
    return { x: 50 + Math.cos(r) * c.s * 42, y: 50 - Math.sin(r) * c.s * 42 }
  }
  /** @param {PointerEvent} e */
  function padPoint(e) {
    if (!pad) return
    const b = pad.getBoundingClientRect()
    const dx = (e.clientX - (b.left + b.width / 2)) / (b.width * 0.42)
    const dy = -(e.clientY - (b.top + b.height / 2)) / (b.height * 0.42)
    const h = ((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360
    const s = Math.min(1, Math.hypot(dx, dy))
    draft.colors[active] = { h: Math.round(h), s: +s.toFixed(3) }
  }
  /** @param {PointerEvent} e @param {number} [i] */
  function startPad(e, i) {
    if (i !== undefined) active = i
    else {
      // A press on empty pad moves the nearest handle there.
      if (!pad) return
      const b = pad.getBoundingClientRect()
      let best = 0, bestD = Infinity
      draft.colors.forEach((c, k) => {
        const p = at(c)
        const d = Math.hypot(b.left + (p.x / 100) * b.width - e.clientX, b.top + (p.y / 100) * b.height - e.clientY)
        if (d < bestD) { bestD = d; best = k }
      })
      active = best
    }
    /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture?.(e.pointerId)
    padPoint(e)
    const move = (/** @type {PointerEvent} */ ev) => padPoint(ev)
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    e.stopPropagation()
  }
  /** Arrow keys nudge the selected handle: ←/→ hue, ↑/↓ strength. @param {KeyboardEvent} e */
  function padKey(e) {
    const c = draft.colors[active]
    if (!c) return
    const step = e.shiftKey ? 10 : 2
    if (e.key === 'ArrowLeft') c.h = (c.h + 360 - step) % 360
    else if (e.key === 'ArrowRight') c.h = (c.h + step) % 360
    else if (e.key === 'ArrowUp') c.s = Math.min(1, c.s + step / 100)
    else if (e.key === 'ArrowDown') c.s = Math.max(0, c.s - step / 100)
    else return
    e.preventDefault()
  }

  function addColor() {
    if (draft.colors.length >= MAX_COLORS) return
    const base = draft.colors[draft.colors.length - 1]
    draft.colors.push({ h: (base.h + 120) % 360, s: Math.max(0.35, base.s) })
    active = draft.colors.length - 1
  }
  function removeColor() {
    if (draft.colors.length <= 1) return
    draft.colors.splice(active, 1)
    active = Math.max(0, active - 1)
  }
  function shuffle() {
    const n = draft.colors.length
    const start = Math.floor(Math.random() * 360)
    draft.colors = Array.from({ length: n }, (_, k) => ({ h: (start + k * (360 / Math.max(2, n + 1))) % 360, s: 0.35 + Math.random() * 0.5 }))
  }

  // ── Swatches ───────────────────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let strip = $state(null)
  const scrollStrip = (/** @type {number} */ dir) => strip?.scrollBy({ left: dir * 160, behavior: 'smooth' })

  // ── Wave slider (intensity) ────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let wave = $state(null)
  const WAVE_W = 300
  const wavePath = $derived.by(() => {
    const amp = 3 + draft.intensity * 9
    const cycles = 7
    let d = ''
    for (let x = 0; x <= WAVE_W; x += 3) {
      const y = 20 + Math.sin((x / WAVE_W) * cycles * Math.PI * 2) * amp
      d += `${x === 0 ? 'M' : 'L'}${x} ${y.toFixed(2)} `
    }
    return d
  })
  /** @param {PointerEvent} e */
  function waveAt(e) {
    if (!wave) return
    const b = wave.getBoundingClientRect()
    draft.intensity = +Math.min(1, Math.max(0, (e.clientX - b.left) / b.width)).toFixed(3)
  }
  /** @param {PointerEvent} e */
  function startWave(e) {
    waveAt(e)
    const move = (/** @type {PointerEvent} */ ev) => waveAt(ev)
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }

  // ── Opacity (alpha) ────────────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let alphaEl = $state(null)
  /** @param {PointerEvent} e */
  function alphaAt(e) {
    if (!alphaEl) return
    const b = alphaEl.getBoundingClientRect()
    const f = Math.min(1, Math.max(0, (e.clientX - b.left) / b.width))
    draft.alpha = +(0.3 + f * 0.7).toFixed(3)
  }
  /** @param {PointerEvent} e */
  function startAlpha(e) {
    alphaAt(e)
    const move = (/** @type {PointerEvent} */ ev) => alphaAt(ev)
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }
  /** @param {KeyboardEvent} e */
  function alphaKey(e) {
    const step = e.shiftKey ? 0.1 : 0.02
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') draft.alpha = Math.min(1, +(draft.alpha + step).toFixed(3))
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') draft.alpha = Math.max(0.3, +(draft.alpha - step).toFixed(3))
    else return
    e.preventDefault()
  }

  // ── Dial (grain) ───────────────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let dial = $state(null)
  const TICKS = 24
  /** The dial sweeps 270°, from 7:30 to 4:30 like a hardware knob. */
  const dialAngle = $derived(-135 + draft.noise * 270)
  /** @param {PointerEvent} e */
  function dialAt(e) {
    if (!dial) return
    const b = dial.getBoundingClientRect()
    // Angle from 12 o'clock, clockwise.
    let a = (Math.atan2(e.clientX - (b.left + b.width / 2), -(e.clientY - (b.top + b.height / 2))) * 180) / Math.PI
    a = Math.min(135, Math.max(-135, a))
    draft.noise = +((a + 135) / 270).toFixed(3)
  }
  /** @param {PointerEvent} e */
  function startDial(e) {
    dialAt(e)
    const move = (/** @type {PointerEvent} */ ev) => dialAt(ev)
    const up = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up) }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
  }
  /** Shared arrow-key handling for the two 0..1 sliders. @param {KeyboardEvent} e @param {'intensity' | 'noise'} key */
  function sliderKey(e, key) {
    const step = e.shiftKey ? 0.1 : 0.02
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') draft[key] = Math.min(1, +(draft[key] + step).toFixed(3))
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') draft[key] = Math.max(0, +(draft[key] - step).toFixed(3))
    else if (e.key === 'Home') draft[key] = 0
    else if (e.key === 'End') draft[key] = 1
    else return
    e.preventDefault()
  }

  const MODES = /** @type {const} */ ([
    { id: 'auto', label: 'Match system', icon: Sparkles },
    { id: 'light', label: 'Light', icon: Sun },
    { id: 'dark', label: 'Dark', icon: Moon },
  ])

  const NOISE_SVG = `url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='120' height='120'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='1.1' numOctaves='3'/></filter><rect width='100%' height='100%' filter='url(%23n)' opacity='0.55'/></svg>")`
</script>

<Dialog.Root bind:open {onOpenChange}>
  <Dialog.Content showCloseButton={false} class="w-full max-w-sm gap-0 overflow-hidden p-0">
    <Dialog.Title class="sr-only">Theme builder</Dialog.Title>
    <Dialog.Description class="sr-only">Pick colours, intensity and grain for the Custom theme.</Dialog.Description>

    <div class="flex flex-col gap-4 p-4">
      <!-- Mode -->
      <div class="flex items-center justify-center gap-2" role="radiogroup" aria-label="Appearance">
        {#each MODES as m (m.id)}
          {@const Glyph = m.icon}
          <button
            type="button"
            role="radio"
            aria-checked={draft.mode === m.id}
            title={m.label}
            aria-label={m.label}
            class={cn(
              'inline-flex size-9 items-center justify-center rounded-md transition-colors',
              draft.mode === m.id ? 'bg-muted text-foreground' : 'text-muted-foreground hover:bg-muted/60 hover:text-foreground',
            )}
            onclick={() => (draft.mode = m.id)}
          ><Glyph class="size-4 shrink-0" /></button>
        {/each}
      </div>

      <!-- Pad -->
      <div
        bind:this={pad}
        role="slider"
        tabindex="0"
        aria-label="Colour: drag a handle; angle is hue, distance from the centre is strength"
        aria-valuetext={`Hue ${draft.colors[active]?.h ?? 0}°, strength ${Math.round((draft.colors[active]?.s ?? 0) * 100)}%`}
        aria-valuenow={draft.colors[active]?.h ?? 0}
        class="relative aspect-square w-full cursor-crosshair touch-none select-none overflow-hidden rounded-lg border border-border/60 bg-muted/40 outline-none focus-visible:ring-2 focus-visible:ring-ring"
        style="background-image: radial-gradient(circle, color-mix(in oklch, var(--foreground) 16%, transparent) 1px, transparent 1.2px), conic-gradient(from 90deg, oklch(0.7 0.12 0 / 0.10), oklch(0.7 0.12 60 / 0.10), oklch(0.7 0.12 120 / 0.10), oklch(0.7 0.12 180 / 0.10), oklch(0.7 0.12 240 / 0.10), oklch(0.7 0.12 300 / 0.10), oklch(0.7 0.12 360 / 0.10)); background-size: 14px 14px, 100% 100%;"
        onpointerdown={(e) => startPad(e)}
        onkeydown={padKey}
      >
        {#if draft.colors.length > 1}
          <svg class="pointer-events-none absolute inset-0 size-full" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
            <polyline
              points={draft.colors.map((c) => `${at(c).x},${at(c).y}`).join(' ')}
              fill="none"
              stroke="currentColor"
              stroke-width="0.5"
              stroke-dasharray="1.2 1.6"
              class="text-foreground/40"
              vector-effect="non-scaling-stroke"
            />
          </svg>
        {/if}
        {#each draft.colors as c, i (i)}
          {@const p = at(c)}
          <button
            type="button"
            aria-label={`Colour ${i + 1}`}
            class={cn(
              'absolute -translate-x-1/2 -translate-y-1/2 rounded-full border-white shadow-[0_2px_10px_rgb(0_0_0/0.35)] transition-[width,height,border-width] duration-150',
              i === active ? 'size-12 border-[5px]' : 'size-8 border-[3px]',
            )}
            style="left: {p.x}%; top: {p.y}%; background: {swatchCss(c)};"
            onpointerdown={(e) => startPad(e, i)}
          ></button>
        {/each}
      </div>

      <!-- Add / remove / shuffle -->
      <div class="flex items-center justify-center gap-6 text-muted-foreground">
        <button type="button" title="Add a colour" aria-label="Add a colour" disabled={draft.colors.length >= MAX_COLORS} class="inline-flex size-8 items-center justify-center rounded-md transition-colors hover:bg-muted/60 hover:text-foreground disabled:opacity-30" onclick={addColor}><Plus class="size-4 shrink-0" /></button>
        <button type="button" title="Remove the selected colour" aria-label="Remove the selected colour" disabled={draft.colors.length <= 1} class="inline-flex size-8 items-center justify-center rounded-md transition-colors hover:bg-muted/60 hover:text-foreground disabled:opacity-30" onclick={removeColor}><Minus class="size-4 shrink-0" /></button>
        <button type="button" title="Shuffle" aria-label="Shuffle colours" class="inline-flex size-8 items-center justify-center rounded-md transition-colors hover:bg-muted/60 hover:text-foreground" onclick={shuffle}><Shuffle class="size-4 shrink-0" /></button>
      </div>

      <!-- Swatches -->
      <div class="flex items-center gap-1">
        <button type="button" aria-label="Earlier colours" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" onclick={() => scrollStrip(-1)}><ChevronLeft class="size-4 shrink-0" /></button>
        <div bind:this={strip} class="no-scrollbar flex min-w-0 flex-1 gap-2 overflow-x-auto py-1">
          {#each PRESETS as p (p.name)}
            <button
              type="button"
              title={p.name}
              aria-label={p.name}
              class="size-8 shrink-0 rounded-full border border-black/10 shadow-sm transition-transform hover:scale-110"
              style="background: {swatchCss(p)}"
              onclick={() => (draft.colors[active] = { h: p.h, s: p.s })}
            ></button>
          {/each}
        </div>
        <button type="button" aria-label="More colours" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" onclick={() => scrollStrip(1)}><ChevronRight class="size-4 shrink-0" /></button>
      </div>

      <!-- Opacity: below 100% the window turns to glass (macOS) -->
      <div class="flex items-center gap-3">
        <span class="w-14 shrink-0 text-ui-xs text-muted-foreground">Opacity</span>
        <div
          bind:this={alphaEl}
          role="slider"
          tabindex="0"
          aria-label="Window opacity"
          aria-valuemin="30"
          aria-valuemax="100"
          aria-valuenow={Math.round(draft.alpha * 100)}
          class="relative h-6 min-w-0 flex-1 cursor-ew-resize touch-none select-none rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring"
          style="background-image: linear-gradient(90deg, transparent, {swatchCss(draft.colors[0])}), repeating-conic-gradient(color-mix(in oklch, var(--foreground) 14%, transparent) 0 25%, transparent 0 50%); background-size: 100% 100%, 10px 10px;"
          onpointerdown={startAlpha}
          onkeydown={alphaKey}
        >
          <span
            class="pointer-events-none absolute top-1/2 size-5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white bg-white/30 shadow-[0_1px_6px_rgb(0_0_0/0.35)]"
            style="left: {((draft.alpha - 0.3) / 0.7) * 100}%"
          ></span>
        </div>
        <span class="w-9 shrink-0 text-right text-ui-2xs tabular-nums text-muted-foreground">{Math.round(draft.alpha * 100)}%</span>
      </div>

      <!-- Intensity wave + grain dial -->
      <div class="flex items-center gap-4">
        <div
          bind:this={wave}
          role="slider"
          tabindex="0"
          aria-label="Intensity"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(draft.intensity * 100)}
          class="relative h-12 min-w-0 flex-1 cursor-ew-resize touch-none select-none rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onpointerdown={startWave}
          onkeydown={(e) => sliderKey(e, 'intensity')}
        >
          <svg class="absolute inset-0 size-full" viewBox="0 0 {WAVE_W} 40" preserveAspectRatio="none" aria-hidden="true">
            <defs>
              <clipPath id="tb-wave-on"><rect x="0" y="0" width={draft.intensity * WAVE_W} height="40" /></clipPath>
            </defs>
            <path d={wavePath} fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" class="text-foreground/20" vector-effect="non-scaling-stroke" />
            <path d={wavePath} fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" class="text-foreground/60" clip-path="url(#tb-wave-on)" vector-effect="non-scaling-stroke" />
          </svg>
          <span
            class="pointer-events-none absolute top-0 h-12 w-5 -translate-x-1/2 rounded-full bg-white shadow-[0_2px_8px_rgb(0_0_0/0.3)]"
            style="left: {draft.intensity * 100}%"
          ></span>
        </div>

        <div
          bind:this={dial}
          role="slider"
          tabindex="0"
          aria-label="Grain"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(draft.noise * 100)}
          title="Grain"
          class="relative size-20 shrink-0 cursor-grab touch-none select-none rounded-full outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
          onpointerdown={startDial}
          onkeydown={(e) => sliderKey(e, 'noise')}
        >
          {#each Array.from({ length: TICKS }) as _, k (k)}
            {@const a = -135 + (k / (TICKS - 1)) * 270}
            {@const lit = (k / (TICKS - 1)) <= draft.noise + 0.0001}
            <span
              class={cn('absolute left-1/2 top-1/2 size-1 rounded-full', lit ? 'bg-foreground/70' : 'bg-foreground/20')}
              style="transform: translate(-50%, -50%) rotate({a}deg) translateY(-36px);"
            ></span>
          {/each}
          <span
            class="absolute inset-3 rounded-full border border-border/60 bg-muted shadow-[inset_0_1px_0_rgb(255_255_255/0.08),0_2px_6px_rgb(0_0_0/0.3)]"
            style="background-image: {NOISE_SVG}; transform: rotate({dialAngle}deg);"
          >
            <span class="absolute left-1/2 top-1.5 h-2.5 w-1 -translate-x-1/2 rounded-full bg-foreground/80"></span>
          </span>
        </div>
      </div>
    </div>

    {#if importing}
      <div class="flex items-center gap-2 border-t border-border/50 px-4 py-2.5">
        <input
          type="text"
          bind:value={importText}
          placeholder="Paste a stroke-theme: code"
          aria-label="Theme code"
          class="h-8 min-w-0 flex-1 px-2.5 font-mono text-ui-xs"
          spellcheck="false"
          autocomplete="off"
          onpaste={(e) => { const t = e.clipboardData?.getData('text') ?? ''; if (t) { e.preventDefault(); importCode(t) } }}
          onkeydown={(e) => { if (e.key === 'Enter') importCode(importText); else if (e.key === 'Escape') { e.stopPropagation(); importing = false } }}
        />
        <Button type="button" size="sm" variant="outline" onclick={() => importCode(importText)}>Load</Button>
      </div>
    {/if}
    <div class="flex items-center gap-2 border-t border-border/50 bg-muted/20 px-4 py-3">
      <Button type="button" variant="ghost" size="sm" onclick={copyCode} title="Copy this theme as a code to share">Copy code</Button>
      <Button type="button" variant="ghost" size="sm" onclick={() => (importing = !importing)} aria-pressed={importing}>Import</Button>
      <span class="flex-1"></span>
      <Button type="button" variant="ghost" size="sm" onclick={() => close(false)}>Cancel</Button>
      <Button type="button" size="sm" onclick={() => close(true)}>Apply</Button>
    </div>
  </Dialog.Content>
</Dialog.Root>
