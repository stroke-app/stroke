<script>
  // Aggregation pipeline builder: a list of stages (operator, JSON5 body, on/off),
  // each with a live preview of the documents coming out of it. Run full shows
  // the result in the grid; pipelines save per collection.
  import { untrack } from 'svelte'
  import { cn } from '$lib/utils.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { docGrid } from '$lib/doc-adapters.js'
  import DataTable from './DataTable.svelte'
  import Plus from '@lucide/svelte/icons/plus'
  import X from '@lucide/svelte/icons/x'
  import ArrowUp from '@lucide/svelte/icons/arrow-up'
  import ArrowDown from '@lucide/svelte/icons/arrow-down'
  import Play from '@lucide/svelte/icons/play'
  import Copy from '@lucide/svelte/icons/copy'
  import TerminalIcon from '@lucide/svelte/icons/terminal'
  import Save from '@lucide/svelte/icons/save'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import MoreHorizontal from '@lucide/svelte/icons/ellipsis'

  /**
   * @type {{
   *   adapter: import('$lib/doc-adapters.js').DocAdapter,
   *   coll: string,
   *   connId?: string,
   *   onopensql?: (text: string, collection: string) => void,
   * }}
   */
  let { adapter, coll, connId = '', onopensql = () => {} } = $props()

  const OPS = ['$match', '$project', '$group', '$sort', '$limit', '$skip', '$unwind', '$lookup', '$addFields', '$set', '$unset', '$count', '$sample', '$facet', '$replaceRoot']
  /** @type {Record<string, string>} */
  const SEEDS = {
    $match: '{}',
    $project: '{_id: 1}',
    $group: '{_id: "$field", count: {$sum: 1}}',
    $sort: '{_id: 1}',
    $limit: '10',
    $skip: '0',
    $unwind: '"$field"',
    $lookup: '{from: "other", localField: "field", foreignField: "_id", as: "joined"}',
    $addFields: '{newField: "$field"}',
    $set: '{newField: "$field"}',
    $unset: '"field"',
    $count: '"count"',
    $sample: '{size: 10}',
    $facet: '{byField: [{$sortByCount: "$field"}]}',
    $replaceRoot: '{newRoot: "$field"}',
  }
  const PREVIEW = 10

  /** @typedef {{ id: number, op: string, body: string, on: boolean, open: boolean }} Stage */
  let nextId = 1
  /** @param {string} op @param {string} [body] @returns {Stage} */
  const mk = (op, body) => ({ id: nextId++, op, body: body ?? SEEDS[op] ?? '{}', on: true, open: true })

  let stages = $state(/** @type {Stage[]} */ ([mk('$match')]))
  /** Per stage id: preview docs, error, loading. */
  let previews = $state(/** @type {Record<number, { docs?: any[], error?: string, loading?: boolean }>} */ ({}))

  // ── Saved pipelines ──────────────────────────────────────────────────────
  const storeKey = $derived(`stroke:mongo:pipelines:${connId}:${coll}`)
  /** @returns {Record<string, { op: string, body: string, on: boolean }[]>} */
  function readSaved() {
    try {
      const v = JSON.parse(localStorage.getItem(storeKey) ?? '{}')
      return v && typeof v === 'object' && !Array.isArray(v) ? v : {}
    } catch {
      return {}
    }
  }
  let saved = $state(/** @type {Record<string, { op: string, body: string, on: boolean }[]>} */ ({}))
  $effect(() => {
    void storeKey
    saved = readSaved()
    untrack(() => {
      stages = [mk('$match')]
      previews = {}
      results = null
      loadedName = ''

    })
  })
  /** The saved pipeline last opened or saved. */
  let loadedName = $state('')
  let naming = $state(false)
  let saveName = $state('')
  function savePipeline() {
    const name = saveName.trim()
    naming = false
    saveName = ''
    if (!name) return
    const next = { ...readSaved(), [name]: stages.map(({ op, body, on }) => ({ op, body, on })) }
    try {
      localStorage.setItem(storeKey, JSON.stringify(next))
      saved = next
      loadedName = name
      toast.success(`Saved "${name}"`)
    } catch (e) {
      toast.error('Could not save the pipeline', { description: e instanceof Error ? e.message : String(e) })
    }
  }
  /** @param {string} name */
  function loadPipeline(name) {
    const p = saved[name]
    if (!p) return
    loadedName = name
    stages = p.map((s) => ({ ...mk(s.op, s.body), on: s.on !== false }))
    previews = {}
  }
  /** @param {string} name */
  function deleteSaved(name) {
    const next = { ...readSaved() }
    delete next[name]
    try {
      localStorage.setItem(storeKey, JSON.stringify(next))
    } catch {}
    saved = next
    loadedName = ''
  }

  // ── Text ─────────────────────────────────────────────────────────────────
  /** @param {Stage[]} list */
  const pipelineText = (list) => `[${list.filter((s) => s.on).map((s) => `{${s.op}: ${s.body.trim() || '{}'}}`).join(', ')}]`
  /** @param {Stage[]} list */
  const prettyPipeline = (list) => `[\n${list.filter((s) => s.on).map((s) => `  { ${s.op}: ${s.body.trim() || '{}'} }`).join(',\n')}\n]`
  const shellRef = $derived.by(() => {
    const name = coll.slice(coll.indexOf('.') + 1)
    return /^[A-Za-z_$][\w$]*$/.test(name) ? `db.${name}` : `db.getCollection(${JSON.stringify(name)})`
  })
  const mongosh = $derived(`${shellRef}.aggregate(${prettyPipeline(stages)})`)

  async function copyShell() {
    try {
      await navigator.clipboard.writeText(mongosh)
      toast.success('Copied as mongosh')
    } catch (e) {
      toast.error('Could not copy', { description: e instanceof Error ? e.message : String(e) })
    }
  }

  // ── Previews (debounced) ───────────────────────────────────────────────────
  let timer = /** @type {ReturnType<typeof setTimeout> | undefined} */ (undefined)
  let gen = 0
  $effect(() => {
    // Track every stage's op, body and switch.
    const snap = stages.map((s) => `${s.id}|${s.op}|${s.on}|${s.body}`).join('\n')
    void snap
    clearTimeout(timer)
    timer = setTimeout(() => untrack(() => void refreshPreviews()), 400)
    return () => clearTimeout(timer)
  })

  async function refreshPreviews() {
    if (!adapter.aggregate) return
    const g = ++gen
    const list = $state.snapshot(stages)
    /** @type {typeof previews} */
    const next = {}
    for (const s of list) next[s.id] = { ...(previews[s.id] ?? {}), loading: s.on }
    previews = next
    // Each enabled stage previews the pipeline up to and including itself;
    // after the first failure the later ones wait for it to be fixed.
    let failed = false
    for (let i = 0; i < list.length; i++) {
      const s = list[i]
      if (!s.on) continue
      if (failed) {
        previews = { ...previews, [s.id]: { error: 'Fix the stage above first' } }
        continue
      }
      try {
        const docs = await adapter.aggregate(coll, pipelineText(list.slice(0, i + 1)), PREVIEW)
        if (g !== gen) return
        previews = { ...previews, [s.id]: { docs } }
      } catch (e) {
        if (g !== gen) return
        failed = true
        previews = { ...previews, [s.id]: { error: e instanceof Error ? e.message : String(e) } }
      }
    }
  }

  // ── Stage list ───────────────────────────────────────────────────────────
  /** @param {number} i @param {number} d */
  function move(i, d) {
    const j = i + d
    if (j < 0 || j >= stages.length) return
    const next = [...stages]
    ;[next[i], next[j]] = [next[j], next[i]]
    stages = next
  }
  /** @param {number} i */
  function remove(i) {
    stages = stages.filter((_, j) => j !== i)
  }
  function add() {
    stages = [...stages, mk(stages.length ? '$project' : '$match')]
  }
  /** @param {Stage} s @param {string} op */
  function setOp(s, op) {
    // Swap the seed body only while it is still the previous seed.
    if (s.body.trim() === (SEEDS[s.op] ?? '').trim()) s.body = SEEDS[op] ?? '{}'
    s.op = op
  }

  // ── Run full ───────────────────────────────────────────────────────────────
  let results = $state.raw(/** @type {Record<string, any>[] | null} */ (null))
  let running = $state(false)
  let resultVersion = $state(0)
  const RUN_CAP = 1000
  async function runFull() {
    if (!adapter.aggregate) return
    running = true
    try {
      results = await adapter.aggregate(coll, pipelineText(stages), RUN_CAP)
      resultVersion += 1
    } catch (e) {
      toast.error('The pipeline failed', { description: e instanceof Error ? e.message : String(e) })
    } finally {
      running = false
    }
  }
  const resultGrid = $derived(results ? docGrid(results, ['_id']) : null)

  /** @param {any} d */
  const oneLine = (d) => {
    const s = JSON.stringify(d)
    return s.length > 240 ? s.slice(0, 240) + '…' : s
  }
  /** A short pretty-printed doc: up to 8 lines. @param {any} d */
  const previewText = (d) => {
    const lines = JSON.stringify(d, null, 2).split('\n')
    return lines.length > 8 ? lines.slice(0, 8).join('\n') + '\n  …' : lines.join('\n')
  }
  const iconBtn = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-30'
  const pill = 'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2.5 text-ui-xs text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent/50 hover:text-foreground disabled:opacity-40'
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <!-- Toolbar: what the pipeline is, then quiet icon actions and one Run. -->
  <div class="flex h-11 shrink-0 items-center gap-1 border-b border-border/30 pl-4 pr-2">
    <span class="text-ui-xs text-muted-foreground tabular-nums">
      {#if loadedName}<span class="text-foreground">{loadedName}</span> · {/if}{stages.length} {stages.length === 1 ? 'stage' : 'stages'}
    </span>
    <span class="flex-1"></span>
    {#if naming}
      <form class="flex items-center" onsubmit={(e) => { e.preventDefault(); savePipeline() }}>
        <!-- svelte-ignore a11y_autofocus -->
        <input bind:value={saveName} autofocus placeholder="Pipeline name" aria-label="Pipeline name" onkeydown={(e) => { if (e.key === 'Escape') naming = false }} onblur={() => { if (!saveName.trim()) naming = false }} class="field-surface h-7 w-44 px-2.5 text-ui-xs text-foreground shadow-none outline-none placeholder:text-muted-foreground" />
      </form>
    {/if}
    <DropdownMenu.Root>
      <DropdownMenu.Trigger class={iconBtn} title="Pipeline actions" aria-label="Pipeline actions">
        <MoreHorizontal class="size-3.5 shrink-0" />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="min-w-56">
        <DropdownMenu.Item onSelect={() => (naming = true)}><Save class="size-3.5 shrink-0" /><span data-slot="menu-label">Save pipeline…</span></DropdownMenu.Item>
        {#if Object.keys(saved).length}
          <DropdownMenu.Sub>
            <DropdownMenu.SubTrigger><FolderOpen class="size-3.5 shrink-0" /><span data-slot="menu-label">Open saved</span></DropdownMenu.SubTrigger>
            <DropdownMenu.SubContent class="min-w-44">
              {#each Object.keys(saved) as n (n)}
                <DropdownMenu.Item onSelect={() => loadPipeline(n)}><span data-slot="menu-label" class="truncate">{n}</span></DropdownMenu.Item>
              {/each}
            </DropdownMenu.SubContent>
          </DropdownMenu.Sub>
        {/if}
        <DropdownMenu.Separator />
        <DropdownMenu.Item onSelect={() => void copyShell()}><Copy class="size-3.5 shrink-0" /><span data-slot="menu-label">Copy as mongosh</span></DropdownMenu.Item>
        <DropdownMenu.Item onSelect={() => onopensql(mongosh, coll)}><TerminalIcon class="size-3.5 shrink-0" /><span data-slot="menu-label">Open in console</span></DropdownMenu.Item>
        {#if loadedName && saved[loadedName]}
          <DropdownMenu.Separator />
          <DropdownMenu.Item variant="destructive" onSelect={() => deleteSaved(loadedName)}><X class="size-3.5 shrink-0" /><span data-slot="menu-label" class="truncate">Delete “{loadedName}”</span></DropdownMenu.Item>
        {/if}
      </DropdownMenu.Content>
    </DropdownMenu.Root>
    <button
      type="button"
      class="ml-1 inline-flex h-7 items-center gap-1.5 rounded-full bg-foreground px-3.5 text-ui-xs font-medium text-background transition-opacity hover:opacity-90 disabled:opacity-40"
      onclick={() => void runFull()}
      disabled={running || !stages.some((s) => s.on)}
      title="Run the whole pipeline (first {RUN_CAP.toLocaleString()} results)"
    >
      {#if running}<LoaderCircle class="size-3.5 shrink-0 animate-spin" />{:else}<Play class="size-3.5 shrink-0" />{/if} Run
    </button>
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-auto px-4 py-4">
    <ol class="flex flex-col">
      {#each stages as s, i (s.id)}
        {@const p = previews[s.id]}
        <li class={cn('group/stage relative flex flex-col pb-5 pl-9 transition-opacity', !s.on && 'opacity-55')}>
          <!-- Rail: the stage number, joined to the next by a thin line. -->
          <span class="absolute left-0 top-1 grid size-6 place-items-center rounded-full bg-muted text-ui-2xs tabular-nums text-foreground/80">{i + 1}</span>
          {#if i < stages.length - 1}<span class="absolute bottom-0 left-3 top-8 w-px bg-border/50" aria-hidden="true"></span>{/if}

          <div class="flex h-8 items-center gap-2">
            <Select.Root type="single" value={s.op} onValueChange={(v) => { if (v) setOp(s, v) }}>
              <Select.Trigger size="sm" class="h-7 w-auto gap-1.5 rounded-md border-transparent! bg-transparent! px-2 font-mono text-ui-sm font-medium text-foreground shadow-none hover:bg-accent/50!" aria-label="Stage operator">{s.op}</Select.Trigger>
              <Select.Content>
                {#each OPS as op (op)}<Select.Item value={op} label={op} class="font-mono" />{/each}
              </Select.Content>
            </Select.Root>
            {#if !s.open}<span class="min-w-0 flex-1 truncate font-mono text-ui-2xs text-muted-foreground">{s.body.replace(/\s+/g, ' ')}</span>{:else}<span class="flex-1"></span>{/if}
            {#if p?.loading}<LoaderCircle class="size-3.5 shrink-0 animate-spin text-muted-foreground" />{:else if p?.docs && s.on}<span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground">{p.docs.length}{p.docs.length === PREVIEW ? '+' : ''} {p.docs.length === 1 ? 'doc' : 'docs'} out</span>{/if}
            <Switch checked={s.on} onCheckedChange={(v) => (s.on = v)} aria-label="Enable stage" title={s.on ? 'Turn this stage off' : 'Turn this stage on'} />
            <DropdownMenu.Root>
              <DropdownMenu.Trigger class={cn(iconBtn, 'opacity-0 group-hover/stage:opacity-100 data-[state=open]:opacity-100')} aria-label="Stage actions">
                <MoreHorizontal class="size-3.5 shrink-0" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end" class="min-w-44">
                <DropdownMenu.Item onSelect={() => (s.open = !s.open)}><ChevronRight class={cn('size-3.5 shrink-0', s.open && 'rotate-90')} /><span data-slot="menu-label">{s.open ? 'Collapse' : 'Expand'}</span></DropdownMenu.Item>
                <DropdownMenu.Item disabled={i === 0} onSelect={() => move(i, -1)}><ArrowUp class="size-3.5 shrink-0" /><span data-slot="menu-label">Move up</span></DropdownMenu.Item>
                <DropdownMenu.Item disabled={i === stages.length - 1} onSelect={() => move(i, 1)}><ArrowDown class="size-3.5 shrink-0" /><span data-slot="menu-label">Move down</span></DropdownMenu.Item>
                <DropdownMenu.Separator />
                <DropdownMenu.Item variant="destructive" onSelect={() => remove(i)}><X class="size-3.5 shrink-0" /><span data-slot="menu-label">Delete stage</span></DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </div>

          {#if s.open}
            <!-- Body beside its output, as in Compass: write on the left, see what comes out on the right. -->
            <div class="mt-1.5 grid grid-cols-1 gap-3 lg:grid-cols-2">
              <div class="flex h-36 min-h-0 overflow-hidden rounded-xl bg-muted/30">
                {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
                  <CodeEditor bind:value={s.body} lang="json" gutter={false} ariaLabel={`Stage ${i + 1} ${s.op}`} />
                {/await}
              </div>
              <div class="app-scroll flex h-36 min-h-0 flex-col gap-1.5 overflow-auto">
                {#if p?.error}
                  <p class="rounded-xl bg-destructive/5 px-3 py-2 text-ui-xs text-destructive">{p.error}</p>
                {:else if !s.on}
                  <p class="px-1 py-2 text-ui-xs text-muted-foreground">Skipped; documents pass through unchanged.</p>
                {:else if p?.docs}
                  {#if p.docs.length}
                    {#each p.docs.slice(0, 5) as d, k (k)}
                      <pre class="shrink-0 overflow-hidden whitespace-pre-wrap break-all rounded-lg bg-muted/20 px-3 py-2 font-mono text-ui-2xs leading-relaxed text-foreground/80" title={JSON.stringify(d, null, 2)}>{previewText(d)}</pre>
                    {/each}
                  {:else}
                    <p class="px-1 py-2 text-ui-xs text-muted-foreground">No documents come out of this stage.</p>
                  {/if}
                {:else}
                  <p class="px-1 py-2 text-ui-xs text-muted-foreground">Preview appears as you type.</p>
                {/if}
              </div>
            </div>
          {/if}
        </li>
      {/each}
    </ol>
    <button type="button" class="ml-9 inline-flex h-8 items-center gap-1.5 rounded-md bg-muted/40 px-3.5 text-ui-xs text-foreground/85 transition-colors hover:bg-accent hover:text-foreground" onclick={add}><Plus class="size-3.5 shrink-0" /> Add stage</button>
  </div>

  {#if resultGrid}
    <section class="flex h-[45%] min-h-40 shrink-0 flex-col border-t border-border/30" aria-label="Pipeline results">
      <div class="flex h-9 shrink-0 items-center gap-2 pl-4 pr-1 text-ui-xs text-muted-foreground">
        <span class="tabular-nums"><span class="text-foreground">{(results?.length ?? 0).toLocaleString()}</span> {results?.length === RUN_CAP ? `results (first ${RUN_CAP.toLocaleString()})` : 'results'}</span>
        <span class="flex-1"></span>
        <button type="button" class={iconBtn} onclick={() => (results = null)} aria-label="Close results" title="Close results"><X class="size-3.5 shrink-0" /></button>
      </div>
      <div class="flex min-h-0 min-w-0 flex-1">
        <DataTable columns={resultGrid.columns} rows={resultGrid.rows} dataVersion={resultVersion} columnWidthsKey={`${connId}\x00agg:${coll}`} readonly />
      </div>
    </section>
  {/if}
</div>
