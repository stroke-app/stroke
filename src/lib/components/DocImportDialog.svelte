<script>
  // Import a JSON / JSON Lines / CSV file into the open collection: pick, look
  // at the first rows, choose typing for CSV, then insert.
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Switch } from '$lib/components/ui/switch/index.js'
  import { guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import FileUp from '@lucide/svelte/icons/file-up'

  /**
   * @type {{
   *   open?: boolean,
   *   adapter: import('$lib/doc-adapters.js').DocAdapter,
   *   coll: string,
   *   readOnly?: boolean,
   *   onimported?: () => void,
   * }}
   */
  let { open = $bindable(false), adapter, coll, readOnly = false, onimported = () => {} } = $props()

  let path = $state('')
  let format = $state('')
  let infer = $state(true)
  let info = $state.raw(/** @type {{ fileName: string, format: string, total: number, preview: Record<string, any>[] } | null} */ (null))
  let error = $state('')
  let busy = $state(false)
  let result = $state.raw(/** @type {{ inserted: number, failed: number, errors: string[] } | null} */ (null))

  /** @param {unknown} e */
  const msg = (e) => (e instanceof Error ? e.message : String(e))
  const FORMATS = [{ v: 'json', l: 'JSON' }, { v: 'jsonl', l: 'JSON Lines' }, { v: 'csv', l: 'CSV' }]

  async function pick() {
    if (!adapter.import) return
    const p = await adapter.import.pick()
    if (!p) return
    path = p
    format = ''
    result = null
    await preview()
  }

  async function preview() {
    if (!adapter.import || !path) return
    busy = true
    error = ''
    try {
      info = await adapter.import.preview(path, { format, infer })
      format = info.format
    } catch (e) {
      info = null
      error = msg(e)
    } finally {
      busy = false
    }
  }

  async function run() {
    if (!adapter.import || !info || !guardWrite('import documents')) return
    busy = true
    error = ''
    try {
      result = await adapter.import.run(coll, path, { format, infer })
      if (result.inserted) {
        toast.success(`Imported ${result.inserted.toLocaleString()} documents`)
        onimported()
      }
    } catch (e) {
      error = msg(e)
    } finally {
      busy = false
    }
  }

  // Start over each time it opens.
  $effect(() => {
    if (!open) return
    path = format = error = ''
    info = result = null
  })

  /** @param {any} d */
  const oneLine = (d) => {
    const s = JSON.stringify(d)
    return s.length > 300 ? s.slice(0, 300) + '…' : s
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-[36rem] max-w-[calc(100vw-2rem)] gap-0 p-0">
    <div class="flex flex-col gap-1 px-5 pt-5 pb-3">
      <Dialog.Title class="text-ui-sm font-semibold leading-none">Import documents</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">Into <span class="font-mono text-foreground">{coll}</span> · JSON, JSON Lines or CSV</Dialog.Description>
    </div>

    <div class="flex flex-col gap-3 px-5 pb-4">
      <button type="button" class="flex h-9 min-w-0 items-center gap-2 rounded-full bg-muted/40 px-3 text-left text-ui-xs transition-colors hover:bg-muted/70" onclick={() => void pick()} disabled={busy}>
        <FileUp class="size-3.5 shrink-0 text-muted-foreground" />
        <span class={cn('min-w-0 flex-1 truncate', info ? 'font-mono text-foreground' : 'text-muted-foreground')}>{info?.fileName ?? (path ? path.split(/[\\/]/).pop() : 'Choose a file…')}</span>
        {#if info}<span class="shrink-0 text-muted-foreground tabular-nums">{info.total.toLocaleString()} documents</span>{/if}
      </button>

      {#if info}
        <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
          <div class="flex items-center" role="group" aria-label="Format">
            {#each FORMATS as f (f.v)}
              <button type="button" aria-pressed={format === f.v} class={cn('inline-flex h-7 items-center rounded-full px-2.5 text-ui-xs transition-colors', format === f.v ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground')} onclick={() => { format = f.v; void preview() }}>{f.l}</button>
            {/each}
          </div>
          {#if format === 'csv'}
            <label class="inline-flex items-center gap-2 text-ui-xs text-foreground">
              <Switch checked={infer} onCheckedChange={(v) => { infer = v; void preview() }} />
              Infer types <span class="text-muted-foreground">(numbers, booleans, dates)</span>
            </label>
          {/if}
        </div>

        <div class="flex flex-col gap-1">
          <span class="text-ui-2xs text-muted-foreground">First {Math.min(5, info.preview.length)} of {info.total.toLocaleString()}</span>
          <ul class="app-scroll flex max-h-48 flex-col gap-0.5 overflow-auto rounded-lg bg-muted/30 p-2">
            {#each info.preview as d, i (i)}
              <li class="truncate font-mono text-ui-2xs text-foreground/80" title={JSON.stringify(d, null, 2)}>{oneLine(d)}</li>
            {:else}
              <li class="text-ui-2xs text-muted-foreground">The file has no documents</li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if error}<p class="rounded-lg bg-destructive/5 px-3 py-1.5 text-ui-2xs text-destructive">{error}</p>{/if}
      {#if result}
        <div class="flex flex-col gap-1 rounded-lg bg-muted/30 px-3 py-2 text-ui-xs">
          <p class="text-foreground">
            Inserted <span class="font-mono tabular-nums">{result.inserted.toLocaleString()}</span>{#if result.failed}, <span class="text-destructive">{result.failed.toLocaleString()} failed</span>{/if}
          </p>
          {#each result.errors as e, i (i)}<p class="truncate font-mono text-ui-2xs text-muted-foreground" title={e}>{e}</p>{/each}
        </div>
      {/if}
    </div>

    <div class="flex items-center justify-end gap-1.5 border-t border-border/30 px-5 py-3">
      <Button variant="ghost" size="sm" class="rounded-full" onclick={() => (open = false)}>{result ? 'Done' : 'Cancel'}</Button>
      {#if !result}
        <Button size="sm" class="rounded-full" onclick={() => void run()} disabled={!info || !info.total || busy || readOnly} title={readOnly ? READ_ONLY_HINT : undefined}>
          {#if busy}<LoaderCircle class="animate-spin" />{/if}
          Import {info ? info.total.toLocaleString() : ''}
        </Button>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
