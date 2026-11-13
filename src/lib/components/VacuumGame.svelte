<script>
  /**
   * VACUUM - the hidden game.
   *
   * You are an autovacuum worker crawling a page of tuples. The toolbar names the
   * status being reclaimed this pass; eat those and grow, eat a tuple that is
   * still visible and the pass aborts.
   *
   * It is drawn as a table because it IS one: a toolbar with the predicate where
   * a filter would be and the count where the row count would be, a header row,
   * a numbered gutter, hairlines in --table-grid, rows flush to both edges. The
   * board sizes itself to the tab the way a real page of rows does - the column
   * count is whatever fits, not a number picked in advance - so it never reads as
   * a widget dropped into the middle of an empty screen.
   */
  import { onMount } from 'svelte'
  import { cn } from '$lib/utils.js'
  import { createGame, turn, step, loadHighScore, saveHighScore, TARGETS } from '$lib/games/vacuum.js'

  /** Matches the grid's own Standard row height, so the board reads at its density. */
  const CELL = 28
  const GUTTER = 44
  const HEADER = 28
  /** Leave room for the toolbar and the hint line without measuring them. */
  const CHROME_H = 36 + 30

  /** @type {HTMLCanvasElement | null} */
  let canvas = $state(null)
  /** @type {HTMLElement | null} */
  let host = $state(null)
  /** @type {HTMLElement | null} */
  let board = $state(null)

  let game = $state(createGame())
  let best = $state(loadHighScore())
  let running = $state(false)
  let started = $state(false)
  /** Set when the predicate changes, so the chip can flash. A rule change you do
   *  not notice is indistinguishable from the game cheating. */
  let flash = $state(false)
  let lastRotations = 0
  /** @type {ReturnType<typeof setTimeout> | null} */
  let flashTimer = null

  /**
   * Column labels.
   *
   * Real column names were the first attempt and they do not fit: `status` and
   * `flags` in a 28px cell at 11px mono overflow into their neighbours, and
   * because the list repeats every ten columns the header came out as a run of
   * collided words - `statusen`, `flagstid`. A grid this narrow labels the way a
   * spreadsheet does, with as many letters as the width can hold.
   * @param {number} i
   */
  function colLabel(i) {
    let n = i
    let out = ''
    do {
      out = String.fromCharCode(65 + (n % 26)) + out
      n = Math.floor(n / 26) - 1
    } while (n >= 0)
    return out
  }

  function palette() {
    const cs = getComputedStyle(document.documentElement)
    const v = (/** @type {string} */ n) => cs.getPropertyValue(n).trim()
    return {
      bg: v('--background'),
      panel: v('--panel'),
      grid: v('--table-grid') || v('--border'),
      muted: v('--muted-foreground'),
      fg: v('--foreground'),
      accent: v('--primary'),
      ok: v('--success'),
      warn: v('--warning'),
      bad: v('--destructive'),
    }
  }

  function statusColor(/** @type {string} */ s, /** @type {ReturnType<typeof palette>} */ p) {
    if (s === 'active') return p.ok
    if (s === 'stale') return p.warn
    if (s === 'locked') return p.bad
    return p.muted
  }

  /** Board dimensions that fill the tab, in whole cells. */
  function fitDims() {
    const w = board?.clientWidth ?? 900
    const h = (host?.clientHeight ?? 600) - CHROME_H
    return {
      cols: Math.max(10, Math.floor((w - GUTTER) / CELL)),
      rows: Math.max(8, Math.floor((h - HEADER) / CELL)),
    }
  }

  function draw() {
    const el = canvas
    if (!el) return
    const ctx = el.getContext('2d')
    if (!ctx) return
    const p = palette()
    const dpr = window.devicePixelRatio || 1
    const w = GUTTER + game.cols * CELL
    const h = HEADER + game.rows * CELL
    if (el.width !== Math.round(w * dpr) || el.height !== Math.round(h * dpr)) {
      el.width = Math.round(w * dpr)
      el.height = Math.round(h * dpr)
      el.style.width = `${w}px`
      el.style.height = `${h}px`
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.fillStyle = p.bg
    ctx.fillRect(0, 0, w, h)

    // Zebra banding, the same cue the grid uses to keep a wide row trackable.
    ctx.fillStyle = p.panel
    ctx.globalAlpha = 0.45
    for (let y = 1; y < game.rows; y += 2) ctx.fillRect(GUTTER, HEADER + y * CELL, w - GUTTER, CELL)
    ctx.globalAlpha = 1

    // Header row + row-number gutter.
    ctx.fillStyle = p.panel
    ctx.fillRect(0, 0, w, HEADER)
    ctx.fillRect(0, 0, GUTTER, h)
    ctx.font = '11px ui-monospace, monospace'
    ctx.textBaseline = 'middle'
    ctx.fillStyle = p.muted
    ctx.textAlign = 'center'
    for (let x = 0; x < game.cols; x++) {
      ctx.fillText(colLabel(x), GUTTER + x * CELL + CELL / 2, HEADER / 2)
    }
    ctx.textAlign = 'right'
    for (let y = 0; y < game.rows; y++) {
      ctx.fillText(String(y + 1), GUTTER - 10, HEADER + y * CELL + CELL / 2)
    }

    ctx.strokeStyle = p.grid
    ctx.lineWidth = 1
    ctx.beginPath()
    for (let x = 0; x <= game.cols; x++) {
      ctx.moveTo(GUTTER + x * CELL + 0.5, HEADER)
      ctx.lineTo(GUTTER + x * CELL + 0.5, h)
    }
    for (let y = 0; y <= game.rows; y++) {
      ctx.moveTo(0, HEADER + y * CELL + 0.5)
      ctx.lineTo(w, HEADER + y * CELL + 0.5)
    }
    ctx.stroke()

    // Tuples. The target is a filled chip, everything else a quiet dot - so
    // "what may I eat" is answerable without reading a word.
    for (let y = 0; y < game.rows; y++) {
      for (let x = 0; x < game.cols; x++) {
        const s = game.grid[y][x]
        if (!s) continue
        const cx = GUTTER + x * CELL + CELL / 2
        const cy = HEADER + y * CELL + CELL / 2
        ctx.fillStyle = statusColor(s, p)
        if (s === game.target) {
          ctx.beginPath()
          ctx.arc(cx, cy, 7.5, 0, Math.PI * 2)
          ctx.fill()
        } else {
          ctx.globalAlpha = 0.45
          ctx.beginPath()
          ctx.arc(cx, cy, 3, 0, Math.PI * 2)
          ctx.fill()
          ctx.globalAlpha = 1
        }
      }
    }

    // The worker, drawn as a run of selected cells - the same shape the grid uses
    // for a selection, so it belongs to the table rather than sitting on it.
    for (let i = game.snake.length - 1; i >= 0; i--) {
      const c = game.snake[i]
      ctx.globalAlpha = i === 0 ? 1 : Math.max(0.3, 1 - i / (game.snake.length + 5))
      ctx.fillStyle = p.accent
      ctx.fillRect(GUTTER + c.x * CELL + 1, HEADER + c.y * CELL + 1, CELL - 2, CELL - 2)
    }
    ctx.globalAlpha = 1
  }

  /** @type {number | null} */
  let raf = null
  let last = 0

  function loop(/** @type {number} */ now) {
    raf = requestAnimationFrame(loop)
    if (!running) return
    if (now - last < game.tickMs) return
    last = now
    step(game)
    if (game.rotations !== lastRotations) {
      lastRotations = game.rotations
      flash = true
      if (flashTimer) clearTimeout(flashTimer)
      flashTimer = setTimeout(() => { flash = false; flashTimer = null }, 900)
    }
    if (game.over) {
      running = false
      best = saveHighScore(game.score)
    }
    draw()
  }

  function start() {
    const { cols, rows } = fitDims()
    game = createGame(Math.random, cols, rows)
    started = true
    running = true
    lastRotations = game.rotations
    flash = false
    last = 0
    draw()
    host?.focus()
  }

  function togglePause() {
    if (!started || game.over) return
    running = !running
  }

  const DIRS = {
    ArrowUp: { x: 0, y: -1 }, ArrowDown: { x: 0, y: 1 },
    ArrowLeft: { x: -1, y: 0 }, ArrowRight: { x: 1, y: 0 },
    w: { x: 0, y: -1 }, s: { x: 0, y: 1 }, a: { x: -1, y: 0 }, d: { x: 1, y: 0 },
  }

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    const dir = DIRS[e.key] ?? DIRS[e.key?.toLowerCase?.()]
    if (dir) {
      e.preventDefault()
      if (!started || game.over) { start(); return }
      running = true
      turn(game, dir)
      return
    }
    if (e.key === ' ') {
      e.preventDefault()
      if (!started || game.over) start()
      else togglePause()
    }
  }

  onMount(() => {
    const { cols, rows } = fitDims()
    game = createGame(Math.random, cols, rows)
    draw()
    raf = requestAnimationFrame(loop)

    // Resize only re-fits the board between runs. Re-fitting mid-run would move
    // the walls while you were driving at them.
    const ro = new ResizeObserver(() => {
      if (!running && !game.over) {
        const d = fitDims()
        game = createGame(Math.random, d.cols, d.rows)
      }
      draw()
    })
    if (host) ro.observe(host)

    // The palette is read from CSS, so a theme swap has to repaint or the board
    // keeps the old colours until the next tick.
    const mo = new MutationObserver(() => draw())
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class'] })

    return () => {
      if (raf) cancelAnimationFrame(raf)
      if (flashTimer) clearTimeout(flashTimer)
      ro.disconnect()
      mo.disconnect()
    }
  })
</script>

<!-- `role="application"` is correct and the linter disagrees: the whole
     interface is arrow keys, so it must hold focus and own its key handling
     rather than letting the app's shortcuts see them. It carries a label and
     full keyboard control, which is what the rule exists to protect. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={host}
  role="application"
  aria-label="VACUUM"
  tabindex="0"
  class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background outline-none"
  onkeydown={onKey}
>
  <!-- Toolbar. Same height, border and surface as the table's: the predicate
       sits where a filter would, the count where the row count would. -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-2 overflow-x-auto border-b border-border bg-panel px-2">
    <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground">VACUUM</span>
    <span
      class={cn(
        'field-surface inline-flex h-7 shrink-0 items-center gap-1.5 px-2.5 font-mono text-ui-2xs transition-colors duration-150',
        flash && 'border-ring bg-accent',
      )}
    >
      <span class="text-muted-foreground">status =</span>
      <span
        class={cn(
          'font-medium',
          game.target === 'active' && 'text-success',
          game.target === 'stale' && 'text-warning',
          game.target === 'locked' && 'text-destructive',
        )}>'{game.target}'</span>
    </span>

    <span class="mx-1 h-4 w-px shrink-0 bg-border" aria-hidden="true"></span>

    {#each TARGETS as t (t)}
      <span class="flex shrink-0 items-center gap-1.5 font-mono text-ui-2xs text-muted-foreground">
        <span
          class={cn(
            'size-2 rounded-full',
            t === 'active' && 'bg-success',
            t === 'stale' && 'bg-warning',
            t === 'locked' && 'bg-destructive',
          )}
        ></span>{t}
      </span>
    {/each}

    <span class="ml-auto flex shrink-0 items-center gap-3 font-mono text-ui-2xs tabular-nums">
      <span class="text-foreground">{game.score}</span>
      <span class="text-muted-foreground">best {best}</span>
    </span>
  </div>

  <!-- Board. Flush to both edges, like a page of rows. -->
  <div bind:this={board} class="relative min-h-0 flex-1 overflow-hidden">
    <canvas bind:this={canvas} class="block"></canvas>

    {#if !started || game.over || !running}
      <div class="absolute inset-0 flex items-center justify-center px-6">
        <div class="flex max-w-lg flex-col items-center gap-3 rounded-lg border border-border bg-background px-8 py-7 text-center">
        {#if game.over}
          <p class="font-mono text-ui-sm font-semibold text-destructive">{game.over.reason}</p>
          <p class="max-w-md font-mono text-ui-2xs text-muted-foreground">{game.over.detail}</p>
          <p class="font-mono text-ui-2xs tabular-nums text-muted-foreground">
            {game.score} reclaimed{#if game.score >= best && game.score > 0} · new best{/if}
          </p>
          <p class="mt-1 font-mono text-ui-2xs text-muted-foreground">
            <kbd>space</kbd> to run again
          </p>
        {:else if !started}
          <p class="font-mono text-ui-sm font-semibold text-foreground">VACUUM</p>
          <!-- Four lines, each one rule, each paired with the mark it describes.
               A paragraph of prose is not how anyone learns a game. -->
          <ul class="flex w-full flex-col gap-2 text-left font-mono text-ui-2xs">
            <li class="flex items-start gap-2.5">
              <span class="mt-1 size-2.5 shrink-0 rounded-full bg-success"></span>
              <span class="text-muted-foreground">
                <span class="text-foreground">Eat the big dots.</span>
                They are the status named in the toolbar. Each one grows you.
              </span>
            </li>
            <li class="flex items-start gap-2.5">
              <span class="mt-1 size-1.5 shrink-0 translate-x-0.5 rounded-full bg-destructive"></span>
              <span class="text-muted-foreground">
                <span class="text-foreground">Avoid the small coloured dots.</span>
                Those tuples are still visible — eating one ends the run.
              </span>
            </li>
            <li class="flex items-start gap-2.5">
              <span class="mt-1 size-1.5 shrink-0 translate-x-0.5 rounded-full bg-muted-foreground"></span>
              <span class="text-muted-foreground">
                <span class="text-foreground">Grey is dead.</span>
                Harmless, worth nothing, clears as you pass over it.
              </span>
            </li>
            <li class="flex items-start gap-2.5">
              <span class="mt-0.5 shrink-0 font-mono text-warning">↻</span>
              <span class="text-muted-foreground">
                <span class="text-foreground">The target changes every four.</span>
                The toolbar chip flashes, and the two cells ahead of you are swept
                so a change can never kill you for a move already made.
              </span>
            </li>
          </ul>
          <p class="mt-1 font-mono text-ui-2xs text-muted-foreground">
            <kbd>space</kbd> to start · arrows or <kbd>wasd</kbd> to move
          </p>
        {:else}
          <p class="font-mono text-ui-sm text-muted-foreground">Paused</p>
          <p class="font-mono text-ui-2xs text-muted-foreground"><kbd>space</kbd> to resume</p>
        {/if}
        </div>
      </div>
    {/if}
  </div>

  <!-- Status line, where the table puts its timing readout. -->
  <div class="flex h-[30px] shrink-0 items-center gap-3 border-t border-border bg-panel px-2 font-mono text-ui-2xs text-muted-foreground">
    <span>{game.snake.length} tuples held</span>
    <span class="ml-auto">{game.cols}×{game.rows} page</span>
  </div>
</div>
