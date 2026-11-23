<script>
  /**
   * Export this app's setup to a file, or import one: connections, saved
   * queries, settings and snippets, with passwords only when asked for and
   * then encrypted with a passphrase (transfer.js). Works without an account
   * or sync. Opened from Settings > Account & Sync and the command palette.
   */
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Checkbox } from '$lib/components/ui/checkbox/index.js'
  import Lock from '@lucide/svelte/icons/lock'
  import FileUp from '@lucide/svelte/icons/file-up'
  import Loader from '@lucide/svelte/icons/loader-2'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { transferDialog } from '$lib/stores/transfer-ui.js'
  import { exportCounts, buildExport, planImport, applyImport } from '$lib/transfer.js'
  import { saveExportAs, openTextFileAs } from '$lib/api.js'
  import { cn } from '$lib/utils.js'

  const open = $derived($transferDialog.open)
  const mode = $derived($transferDialog.mode)
  const close = () => transferDialog.update((s) => ({ ...s, open: false }))

  // ── Export ──
  let counts = $state({ connections: 0, queries: 0, snippets: 0, aiProfiles: 0 })
  let parts = $state({ connections: true, queries: true, settings: true, snippets: true, secrets: false })
  let passphrase = $state('')
  let confirmPass = $state('')
  let busy = $state(false)

  // ── Import ──
  // Raw: the file's objects go to IndexedDB and the keychain as they are, and
  // a reactive proxy cannot be stored ("The object can not be cloned").
  /** @type {import('$lib/transfer-core.js').ImportPlan | null} */
  let plan = $state.raw(null)
  let fileName = $state('')
  let take = $state({ connections: true, queries: true, settings: true, snippets: true })
  let replaceExisting = $state(false)
  let importPass = $state('')
  let importError = $state('')

  $effect(() => {
    if (!open) return
    passphrase = ''
    confirmPass = ''
    importPass = ''
    importError = ''
    busy = false
    if (mode === 'export') {
      parts = { connections: true, queries: true, settings: true, snippets: true, secrets: false }
      void exportCounts().then((c) => (counts = c))
    } else {
      plan = null
      fileName = ''
      replaceExisting = false
      void pickFile()
    }
  })

  const passError = $derived(
    !parts.secrets ? '' : passphrase.length < 8 ? 'At least 8 characters.' : confirmPass && confirmPass !== passphrase ? 'The two do not match.' : '',
  )
  const nothingPicked = $derived(!parts.connections && !parts.queries && !parts.settings && !parts.snippets)
  const canExport = $derived(!busy && !nothingPicked && (!parts.secrets || (!passError && confirmPass === passphrase)))

  async function doExport() {
    if (!canExport) return
    busy = true
    try {
      let version = ''
      try { version = await (await import('@tauri-apps/api/app')).getVersion() } catch { /* dev */ }
      const text = await buildExport({ ...parts, secrets: parts.secrets && (parts.connections || parts.settings) }, passphrase, version)
      const day = new Date().toISOString().slice(0, 10)
      const path = await saveExportAs(text, `stroke-export-${day}.json`, { name: 'Stroke export', extensions: ['json'] })
      if (path) {
        toast.success('Exported', { description: parts.secrets ? `${path} · passwords encrypted with your passphrase` : `${path} · no passwords` })
        close()
      }
    } catch (e) {
      toast.error('Could not export', { description: String(/** @type {any} */ (e)?.message ?? e) })
    } finally {
      busy = false
    }
  }

  async function pickFile() {
    importError = ''
    try {
      const file = await openTextFileAs({ name: 'Stroke export', extensions: ['json'] })
      if (!file) { if (!plan) close(); return }
      const p = planImport(file.content)
      if ('error' in p) { importError = p.error; plan = null; return }
      plan = p
      fileName = file.path.split(/[\\/]/).pop() ?? file.path
      take = { connections: p.connections.length > 0, queries: p.queries > 0, settings: p.settings, snippets: p.snippets > 0 }
    } catch (e) {
      importError = String(e)
    }
  }

  const newCount = $derived(plan ? plan.connections.filter((c) => !c.existing).length : 0)
  const existingCount = $derived(plan ? plan.connections.length - newCount : 0)
  const canImport = $derived(!!plan && !busy && (take.connections || take.queries || take.settings || take.snippets))

  async function doImport() {
    if (!plan || !canImport) return
    busy = true
    importError = ''
    try {
      const r = await applyImport(plan, { ...take, replaceExisting, passphrase: importPass })
      if ('error' in r) { importError = r.error; return }
      const parts = [
        r.connections && `${r.connections} ${r.connections === 1 ? 'connection' : 'connections'}`,
        r.skipped && `${r.skipped} already here, kept`,
        r.queries && `${r.queries} saved ${r.queries === 1 ? 'query' : 'queries'}`,
        r.settings && 'settings',
        r.snippets && `${r.snippets} ${r.snippets === 1 ? 'snippet' : 'snippets'}`,
      ].filter(Boolean)
      toast.success('Imported', { description: `${parts.join(', ')}${plan.encrypted && !r.secrets ? '. Passwords were not imported.' : ''}` })
      close()
    } catch (e) {
      importError = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      busy = false
    }
  }

  const field = 'h-9 w-full rounded-md border border-border bg-background px-2.5 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30'
</script>

{#snippet option(/** @type {string} */ label, /** @type {string} */ hint, /** @type {boolean} */ checked, /** @type {(v: boolean) => void} */ set, /** @type {boolean} */ disabled = false)}
  <label class={cn('flex items-start gap-3 rounded-md px-2 py-2 transition-colors', disabled ? 'opacity-50' : 'cursor-pointer hover:bg-accent/40')}>
    <Checkbox class="mt-0.5" {checked} {disabled} onCheckedChange={(v) => set(!!v)} />
    <span class="min-w-0 flex-1">
      <span class="block text-ui-sm text-foreground">{label}</span>
      <span class="block text-ui-xs text-muted-foreground">{hint}</span>
    </span>
  </label>
{/snippet}

<Dialog.Root open={open} onOpenChange={(v) => { if (!v) close() }}>
  <Dialog.Content class="max-w-[30rem] gap-0 p-0">
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-lg font-semibold">{mode === 'export' ? 'Export connections and settings' : 'Import connections and settings'}</Dialog.Title>
      <Dialog.Description class="text-ui-sm text-muted-foreground">
        {mode === 'export'
          ? 'One file to set up Stroke on another computer. No account needed.'
          : fileName ? fileName : 'Pick a file exported from Stroke.'}
      </Dialog.Description>
    </Dialog.Header>

    <div class="flex flex-col gap-1 px-4 pt-4 pb-2">
      {#if mode === 'export'}
        {@render option('Connections', `${counts.connections} saved, with their workspaces`, parts.connections, (v) => (parts.connections = v), counts.connections === 0)}
        {@render option('Saved queries', `${counts.queries} in the editor's Saved list`, parts.queries, (v) => (parts.queries = v), counts.queries === 0)}
        {@render option('Settings', `Appearance, editor and grid, shortcuts, ${counts.aiProfiles} AI ${counts.aiProfiles === 1 ? 'model' : 'models'}, language`, parts.settings, (v) => (parts.settings = v))}
        {@render option('SQL snippets', `${counts.snippets} of your own`, parts.snippets, (v) => (parts.snippets = v), counts.snippets === 0)}

        <div class="mt-2 rounded-lg border border-border/70 p-3">
          <label class="flex cursor-pointer items-start gap-3">
            <Checkbox class="mt-0.5" checked={parts.secrets} disabled={!parts.connections && !parts.settings} onCheckedChange={(v) => (parts.secrets = !!v)} />
            <span class="min-w-0 flex-1">
              <span class="flex items-center gap-1.5 text-ui-sm text-foreground"><Lock class="size-3.5 shrink-0 text-muted-foreground" />Include passwords and API keys</span>
              <span class="block text-ui-xs text-muted-foreground">Encrypted with a passphrase you choose. Without it, the file has none and each connection asks for its password on the other computer.</span>
            </span>
          </label>
          {#if parts.secrets}
            <div class="mt-3 grid gap-2 ps-7">
              <input type="password" class={field} placeholder="Passphrase" autocomplete="new-password" aria-label="Passphrase" bind:value={passphrase} />
              <input type="password" class={field} placeholder="Passphrase again" autocomplete="new-password" aria-label="Confirm passphrase" bind:value={confirmPass} />
              <p class="text-ui-2xs {passError ? 'text-warning' : 'text-muted-foreground'}">{passError || 'Not stored anywhere: without it the passwords in the file cannot be read.'}</p>
            </div>
          {/if}
        </div>
      {:else if plan}
        {@render option('Connections', plan.connections.length ? `${newCount} new${existingCount ? `, ${existingCount} already here` : ''}` : 'None in this file', take.connections, (v) => (take.connections = v), !plan.connections.length)}
        {#if existingCount && take.connections}
          <label class="ms-9 flex cursor-pointer items-center gap-2 pb-1 text-ui-xs text-muted-foreground">
            <Checkbox checked={replaceExisting} onCheckedChange={(v) => (replaceExisting = !!v)} />
            Replace the {existingCount} already here with the file's copy
          </label>
        {/if}
        {@render option('Saved queries', plan.queries ? `${plan.queries} in the file` : 'None in this file', take.queries, (v) => (take.queries = v), !plan.queries)}
        {@render option('Settings', plan.settings ? 'Replaces appearance, editor, shortcuts, AI models and language here' : 'None in this file', take.settings, (v) => (take.settings = v), !plan.settings)}
        {@render option('SQL snippets', plan.snippets ? `${plan.snippets} in the file; the same name and trigger updates yours` : 'None in this file', take.snippets, (v) => (take.snippets = v), !plan.snippets)}
        {#if plan.encrypted}
          <div class="mt-2 rounded-lg border border-border/70 p-3">
            <p class="flex items-center gap-1.5 text-ui-sm text-foreground"><Lock class="size-3.5 shrink-0 text-muted-foreground" />Passwords and keys are encrypted</p>
            <input type="password" class={cn(field, 'mt-2')} placeholder="Passphrase" aria-label="Passphrase" autocomplete="off" bind:value={importPass} onkeydown={(e) => { if (e.key === 'Enter') void doImport() }} />
            <p class="mt-1.5 text-ui-2xs text-muted-foreground">Leave it empty to import everything else without them.</p>
          </div>
        {/if}
      {:else}
        <div class="flex flex-col items-center gap-3 px-2 py-8 text-center">
          <FileUp class="size-6 shrink-0 text-muted-foreground" />
          <p class="text-ui-sm text-muted-foreground">{importError || 'Choose a file to see what it holds before anything changes.'}</p>
          <Button variant="outline" size="sm" onclick={pickFile}>Choose file…</Button>
        </div>
      {/if}
      {#if mode === 'import' && plan && importError}
        <p class="px-2 pt-2 text-ui-xs text-destructive">{importError}</p>
      {/if}
    </div>

    <Dialog.Footer class="mx-0 mb-0 flex-row items-center gap-2 border-t border-border/40 px-6 py-4 sm:justify-end">
      {#if mode === 'import' && plan}
        <Button variant="ghost" class="me-auto" onclick={pickFile}>Other file…</Button>
      {/if}
      <Button variant="outline" onclick={close}>Cancel</Button>
      {#if mode === 'export'}
        <Button disabled={!canExport} onclick={doExport}>
          {#if busy}<Loader class="size-3.5 shrink-0 animate-spin" />{/if}Export…
        </Button>
      {:else}
        <Button disabled={!canImport} onclick={doImport}>
          {#if busy}<Loader class="size-3.5 shrink-0 animate-spin" />{/if}Import
        </Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
