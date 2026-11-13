<script>
  // A geometry cell, read rather than dumped.
  //
  // The generic editor showed `SRID=3857;POINT(-8252218.02 5023060.07)` - the
  // value, but not the information. What you want from a geometry is where it
  // is and what shape it has: so the default view draws it, names the type and
  // CRS, and (for the SRIDs everyone actually uses) reads the position back as
  // latitude/longitude. The vertices are one click away, the raw EWKT one more.
  import Icon from './Icon.svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import {
    parseGeometry, projectGeometry, geoShapeOf, sridLabel, lonLatOf, formatLonLat, shortCoord,
  } from '$lib/geometry-cell.js'
  import GeoMiniMap from './GeoMiniMap.svelte'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    /** Column name, for the header. */
    column = '',
    /** Column type: `geometry` or `geography` (possibly with type args). */
    dataType = 'geometry',
    nullable = false,
    readOnly = false,
    /** The cell's raw EWKT literal. */
    value = '',
    /** Called with the new literal, or null for SQL NULL. */
    onsave = /** @type {(next: string | null) => void} */ (() => {}),
  } = $props()

  /** @type {'shape' | 'coords' | 'raw'} */
  let tab = $state('shape')
  let draft = $state('')
  let hovered = $state(/** @type {{ x: number, y: number, v: import('$lib/geometry-cell.js').Vertex } | null} */ (null))

  const geom = $derived(parseGeometry(value))
  const dirty = $derived(tab === 'raw' && draft.trim() !== String(value ?? '').trim())

  $effect(() => {
    if (!open) return
    draft = String(value ?? '')
    tab = 'shape'
    hovered = null
  })

  // Drawing surface. Fixed viewBox: the SVG scales with the dialog, the math
  // doesn't have to.
  const W = 600, H = 260
  const proj = $derived(geom && !geom.empty ? projectGeometry(geom, W, H) : null)
  /**
   * The geometry as lon/lat, when it is on Earth. Non-null is what promotes the
   * preview from an abstract plot to a real map - a projected local CRS has no
   * place on one, so those keep the plot.
   */
  const geoShape = $derived(geom ? geoShapeOf(geom) : null)
  // Vertex dots stop earning their pixels on dense geometries; hover lookup
  // stays useful a while longer.
  const showVertexDots = $derived(!!geom && geom.vertexCount <= 400)
  const hoverable = $derived(proj && geom && geom.vertexCount <= 2000 ? proj.vertices : [])

  /** The one-line human reading of the position: lon/lat when derivable. */
  const position = $derived.by(() => {
    if (!geom || !geom.bbox) return null
    const label = geom.type === 'Point' ? 'position' : 'center'
    if (geoShape) {
      const [minLon, minLat, maxLon, maxLat] = geoShape.extent
      return { label, text: formatLonLat((minLon + maxLon) / 2, (minLat + maxLat) / 2) }
    }
    const cx = (geom.bbox.minX + geom.bbox.maxX) / 2
    const cy = (geom.bbox.minY + geom.bbox.maxY) / 2
    const ll = lonLatOf({ x: cx, y: cy }, geom.srid)
    if (!ll) return null
    return { label, text: formatLonLat(ll.lon, ll.lat) }
  })

  const stats = $derived.by(() => {
    if (!geom) return []
    /** @type {Array<{ label: string, value: string, hint?: string, warn?: boolean }>} */
    const out = [
      { label: 'type', value: geom.type + (geom.dims !== 'XY' ? ` ${geom.dims.slice(2)}` : '') },
      { label: 'srid', value: geom.srid == null ? '—' : String(geom.srid), hint: sridLabel(geom.srid), warn: geom.srid == null },
    ]
    if (geom.empty) return out
    out.push({ label: 'vertices', value: geom.vertexCount.toLocaleString() })
    if (geom.parts > 1) out.push({ label: 'parts', value: String(geom.parts) })
    if (geom.bbox && geom.type !== 'Point') {
      const unit = geom.srid === 4326 ? '°' : geom.srid === 3857 ? ' m' : ''
      out.push({
        label: 'extent',
        value: `${shortCoord(geom.bbox.maxX - geom.bbox.minX)}${unit} × ${shortCoord(geom.bbox.maxY - geom.bbox.minY)}${unit}`,
      })
    }
    return out
  })

  /** Vertex list for the Coords tab, flattened but labeled per part. */
  const COORD_CAP = 2000
  const coordParts = $derived.by(() => {
    if (!geom) return []
    /** @type {Array<{ title: string, vertices: import('$lib/geometry-cell.js').Vertex[] }>} */
    const parts = []
    geom.points.forEach((p, i) => parts.push({ title: geom.points.length > 1 ? `Point ${i + 1}` : 'Point', vertices: p }))
    geom.lines.forEach((l, i) => parts.push({ title: geom.lines.length > 1 ? `Line ${i + 1}` : 'Line', vertices: l }))
    geom.polygons.forEach((rings, i) =>
      rings.forEach((r, ri) =>
        parts.push({
          title: `${geom.polygons.length > 1 ? `Polygon ${i + 1} · ` : ''}${ri === 0 ? 'shell' : `hole ${ri}`}`,
          vertices: r,
        })),
    )
    return parts
  })
  const coordsShown = $derived(coordParts.reduce((n, p) => n + p.vertices.length, 0))

  /** @param {MouseEvent} e */
  function onSvgMove(e) {
    if (!hoverable.length) return
    const svg = /** @type {SVGSVGElement} */ (e.currentTarget)
    const r = svg.getBoundingClientRect()
    const mx = ((e.clientX - r.left) / r.width) * W
    const my = ((e.clientY - r.top) / r.height) * H
    let best = null, bestD = 14 * 14 // snap radius in viewBox px
    for (const p of hoverable) {
      const d = (p.x - mx) ** 2 + (p.y - my) ** 2
      if (d < bestD) { bestD = d; best = p }
    }
    hovered = best
  }

  async function copyRaw() {
    try {
      await navigator.clipboard.writeText(String(value ?? ''))
      toast.success('Copied geometry')
    } catch (e) {
      toast.error('Could not copy', { description: String(e) })
    }
  }

  function save() {
    if (readOnly) return
    onsave(draft.trim() === '' ? null : draft)
    open = false
  }
</script>

<Dialog.Root bind:open>
  <!-- Close button lives in the header row, same as the vector viewer, so it
       can't land on top of the tab switcher. -->
  <Dialog.Content showCloseButton={false} class="max-w-2xl gap-0 overflow-hidden p-0">
    <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border/50 px-4">
      <Icon name="globe" class="size-3.5 shrink-0 text-primary" />
      <span class="min-w-0 truncate font-mono text-ui-sm font-medium">{column}</span>
      <span class="shrink-0 rounded bg-muted/60 px-1.5 py-0.5 font-mono text-ui-2xs text-muted-foreground">
        {dataType.replace(/\(.*$/, '')}{#if geom} · {geom.type}{/if}
      </span>
      {#if nullable}
        <span class="shrink-0 rounded bg-muted/40 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">nullable</span>
      {/if}

      <div class="ml-auto flex shrink-0 items-center gap-0.5 rounded-md bg-muted/40 p-0.5">
        {#each [['shape', 'Shape'], ['coords', 'Coords'], ['raw', 'Raw']] as [id, label] (id)}
          <button
            type="button"
            onclick={() => (tab = /** @type {'shape'|'coords'|'raw'} */ (id))}
            class={cn(
              'inline-flex h-6 items-center rounded-md px-2 text-ui-2xs transition-colors',
              tab === id ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
            )}
          >{label}</button>
        {/each}
      </div>

      <Dialog.Close
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none"
        aria-label="Close"
      >
        <Icon name="x" class="size-3.5 shrink-0" />
      </Dialog.Close>
    </div>

    {#if !geom}
      <div class="px-4 py-10 text-center">
        <p class="text-ui-sm text-muted-foreground">This value isn't a geometry literal.</p>
        <p class="mt-1 font-mono text-ui-2xs text-muted-foreground">{String(value ?? '').slice(0, 120)}</p>
      </div>
    {:else}
      <!-- ── Shape ─────────────────────────────────────────────────────────── -->
      {#if tab === 'shape'}
        <div class="px-4 pt-4">
          {#if geom.empty}
            <div class="flex h-40 items-center justify-center rounded-md border border-border/40 bg-muted/10">
              <p class="font-mono text-ui-sm text-muted-foreground">{geom.type} EMPTY</p>
            </div>
          {:else if geoShape}
            <GeoMiniMap shape={geoShape} height={260} />
          {:else if proj}
            <!-- The SVG keeps a fixed aspect box and the hover chip is absolutely
                 positioned inside this wrapper, so hovering never reflows the
                 dialog. -->
            <div class="relative">
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
              <svg
                viewBox="0 0 {W} {H}"
                role="img"
                aria-label="Preview of {geom.type}"
                class="block w-full rounded-md border border-border/40 bg-muted/10"
                onmousemove={onSvgMove}
                onmouseleave={() => (hovered = null)}
              >
                <!-- Graticule: enough structure to read scale and position against. -->
                {#each [1, 2, 3] as i (i)}
                  <line x1={(W / 4) * i} y1="0" x2={(W / 4) * i} y2={H} class="stroke-border/30" stroke-width="1" />
                {/each}
                {#each [1, 2, 3] as i (i)}
                  <line x1="0" y1={(H / 4) * i} x2={W} y2={(H / 4) * i} class="stroke-border/30" stroke-width="1" />
                {/each}

                {#each proj.polygons as d, i (i)}
                  <path {d} fill-rule="evenodd" class="fill-primary/15 stroke-primary/70" stroke-width="1.5" stroke-linejoin="round" />
                {/each}
                {#each proj.lines as d, i (i)}
                  <path {d} fill="none" class="stroke-primary/80" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                {/each}
                {#if showVertexDots}
                  {#each proj.vertices as p, i (i)}
                    <circle cx={p.x} cy={p.y} r="2" class="fill-primary/50" />
                  {/each}
                {/if}
                {#each proj.points as p, i (i)}
                  <circle cx={p.x} cy={p.y} r="9" class="fill-primary/15" />
                  <circle cx={p.x} cy={p.y} r="4" class="fill-primary stroke-background" stroke-width="1.5" />
                {/each}

                {#if hovered}
                  <circle cx={hovered.x} cy={hovered.y} r="5" fill="none" class="stroke-foreground/80" stroke-width="1.5" />
                {/if}
              </svg>
              {#if hovered}
                {@const ll = lonLatOf(hovered.v, geom.srid)}
                <span class="pointer-events-none absolute right-2 top-2 rounded bg-background/90 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground shadow-sm">
                  {ll ? formatLonLat(ll.lon, ll.lat) : `${shortCoord(hovered.v.x)}, ${shortCoord(hovered.v.y)}`}
                </span>
              {/if}
            </div>
          {/if}

          <!-- The human reading of where this is - the payoff line, always fully
               visible rather than squeezed into a truncating tile. -->
          {#if position}
            <div class="mt-2.5 flex items-center gap-1.5 rounded-md border border-border/40 bg-muted/10 px-2.5 py-1.5">
              <Icon name="crosshair" class="size-3.5 shrink-0 text-primary" />
              <span class="text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">{position.label}</span>
              <span class="ml-1 truncate font-mono text-ui-sm tabular-nums text-foreground/90">{position.text}</span>
              {#if geoShape?.assumed}
                <span class="ml-auto shrink-0 text-ui-3xs text-warning">no SRID · read as WGS 84</span>
              {:else if geom.srid === 3857}
                <span class="ml-auto shrink-0 text-ui-3xs text-muted-foreground">from mercator meters</span>
              {/if}
            </div>
          {/if}

          <!-- The figures that identify this geometry at a glance. -->
          <div class="mt-2 grid grid-cols-[repeat(auto-fit,minmax(108px,1fr))] gap-2">
            {#each stats as s (s.label)}
              <div class="rounded-md border border-border/40 bg-muted/10 px-2.5 py-1.5">
                <p class="text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">{s.label}</p>
                <p class="mt-0.5 truncate font-mono text-ui-sm tabular-nums text-foreground/90">{s.value}</p>
                {#if s.hint}
                  <p class={cn('mt-0.5 truncate text-ui-3xs', s.warn ? 'text-warning' : 'text-muted-foreground')}>{s.hint}</p>
                {/if}
              </div>
            {/each}
          </div>
        </div>

      <!-- ── Coords ────────────────────────────────────────────────────────── -->
      {:else if tab === 'coords'}
        <div class="max-h-[22rem] overflow-y-auto px-4 pt-3">
          {#each coordParts as part, pi (pi)}
            {#if coordParts.length > 1}
              <p class="mt-2 mb-1 text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground first:mt-0">{part.title}</p>
            {/if}
            <div class="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-x-2">
              {#each part.vertices.slice(0, COORD_CAP) as v, i (i)}
                <div class="flex items-baseline gap-2 rounded px-1.5 py-0.5 hover:bg-muted/30">
                  <span class="w-9 shrink-0 text-right font-mono text-ui-3xs text-muted-foreground">{i}</span>
                  <span class="truncate font-mono text-ui-2xs tabular-nums text-foreground/85">
                    {shortCoord(v.x)}, {shortCoord(v.y)}{v.z !== undefined ? `, ${shortCoord(v.z)}` : ''}{v.m !== undefined ? ` (m ${shortCoord(v.m)})` : ''}
                  </span>
                </div>
              {/each}
            </div>
          {/each}
          {#if geom.vertexCount > coordsShown || coordParts.some((p) => p.vertices.length > COORD_CAP)}
            <p class="py-2 text-center text-ui-3xs text-muted-foreground">
              Showing the first {COORD_CAP.toLocaleString()} vertices per part - the raw tab has all of them.
            </p>
          {/if}
        </div>

      <!-- ── Raw ───────────────────────────────────────────────────────────── -->
      {:else}
        <div class="px-4 pt-3">
          <textarea
            bind:value={draft}
            readonly={readOnly}
            spellcheck="false"
            class="h-[19rem] w-full resize-none rounded-md border-2 border-border bg-transparent p-3 font-mono text-ui-2xs leading-relaxed text-foreground/90 outline-none transition-colors focus:border-ring/55 read-only:opacity-70"
          ></textarea>
        </div>
      {/if}
    {/if}

    <!-- Footer -->
    <div class="mt-3 flex shrink-0 items-center gap-2 border-t border-border/50 px-4 py-2.5">
      {#if nullable && !readOnly}
        <Button variant="ghost" size="sm" class="h-7 px-2 text-ui-xs text-muted-foreground hover:text-foreground"
          onclick={() => { onsave(null); open = false }}>Set NULL</Button>
      {/if}
      <Button variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-ui-xs text-muted-foreground hover:text-foreground" onclick={copyRaw}>
        <Icon name="copy" class="size-3.5 shrink-0" />
        Copy
      </Button>
      <span class="ml-auto font-mono text-ui-3xs text-muted-foreground">
        {String(value ?? '').length.toLocaleString()} chars
      </span>
      <Button variant="ghost" size="sm" class="h-7 px-2 text-ui-xs" onclick={() => (open = false)}>
        {dirty ? 'Cancel' : 'Close'}
      </Button>
      {#if !readOnly}
        <Button size="sm" class="h-7 px-3 text-ui-xs" disabled={!dirty} onclick={save}>Save</Button>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
