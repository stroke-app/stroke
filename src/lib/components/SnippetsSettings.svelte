<script>
  /**
   * Settings > Snippets: SQL snippets of your own, next to the built-in ones.
   *
   * A snippet is a name, the trigger word typed in the SQL editor, and a body
   * whose `${1:table}` fields Tab walks through. Yours can be limited to some
   * engines, exported to JSON and imported on another machine. The built-in
   * ones are listed read-only below; Duplicate makes an editable copy.
   */
  import { tick } from 'svelte'
  import Plus from '@lucide/svelte/icons/plus'
  import Copy from '@lucide/svelte/icons/copy'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Download from '@lucide/svelte/icons/download'
  import Upload from '@lucide/svelte/icons/upload'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import { Button } from '$lib/components/ui/button/index.js'
  import SearchField from './SearchField.svelte'
  import SelectMenu from './SelectMenu.svelte'
  import CodeEditor from './CodeEditor.svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import { SQL_SNIPPETS } from '$lib/sql-complete-data.js'
  import { saveExportAs, openTextFileAs } from '$lib/api.js'
  import {
    userSnippets,
    saveSnippet,
    removeSnippet,
    SNIPPET_SCOPES,
    TRIGGER_RE,
    snippetsToJson,
    snippetsFromJson,
    importSnippets,
  } from '$lib/stores/sql-snippets.js'

  /** @typedef {import('$lib/stores/sql-snippets.js').UserSnippet} UserSnippet */

  let query = $state('')
  /** '' = every engine. */
  let scopeFilter = $state('')
  let builtinOpen = $state(false)

  /**
   * The snippet being written: a new one (id '') or an edit of one of yours.
   * @type {{ id: string, name: string, trigger: string, body: string, description: string, scope: string[] } | null}
   */
  let draft = $state(null)
  let draftEl = $state(/** @type {HTMLElement | null} */ (null))
  /** Bumped to remount the body editor with a new starting value. */
  let draftKey = $state(0)

  const scopeLabel = (/** @type {string} */ id) => SNIPPET_SCOPES.find((s) => s.id === id)?.label ?? id

  /** @param {{ name: string, trigger: string, body: string, description?: string }} s */
  function matches(s) {
    if (!query.trim()) return true
    const q = query.trim().toLowerCase()
    return [s.name, s.trigger, s.description ?? '', s.body].some((t) => t.toLowerCase().includes(q))
  }

  const mine = $derived(
    $userSnippets.filter((s) => matches(s) && (!scopeFilter || s.scope.length === 0 || s.scope.includes(scopeFilter))),
  )
  const builtins = $derived(
    SQL_SNIPPETS.map((s) => ({ name: s.name, trigger: s.alias, body: s.body, description: '', scope: s.only ?? [] })).filter(
      (s) => matches(s) && (!scopeFilter || s.scope.length === 0 || s.scope.includes(scopeFilter)),
    ),
  )

  /** What is wrong with the draft, or ''. */
  const draftError = $derived.by(() => {
    if (!draft) return ''
    if (!draft.name.trim()) return 'Give it a name.'
    if (!draft.trigger.trim()) return 'Give it a trigger word.'
    if (!TRIGGER_RE.test(draft.trigger.trim())) return 'A trigger is one word: letters, digits, _ or -, starting with a letter.'
    if (!draft.body.trim()) return 'Write the SQL it inserts.'
    return ''
  })
  /** Another snippet of yours with this trigger, for a warning (not an error: both show in the list). */
  const triggerClash = $derived.by(() => {
    if (!draft) return null
    const t = draft.trigger.trim()
    return $userSnippets.find((s) => s.id !== draft?.id && s.trigger === t) ?? null
  })

  /** @param {Partial<UserSnippet>} [from] */
  async function startDraft(from = {}) {
    draft = {
      id: from.id ?? '',
      name: from.name ?? '',
      trigger: from.trigger ?? '',
      body: from.body ?? 'SELECT ${1:*} FROM ${2:table} WHERE ${3:condition}',
      description: from.description ?? '',
      scope: [...(from.scope ?? [])],
    }
    draftKey++
    await tick()
    draftEl?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    draftEl?.querySelector('input')?.focus()
  }

  function commitDraft() {
    if (!draft || draftError) return
    const saved = saveSnippet({ ...draft, id: draft.id || undefined })
    if (saved) {
      toast.success(draft.id ? 'Snippet saved' : 'Snippet added', { description: `Type ${saved.trigger} in the SQL editor to use it.` })
      draft = null
    }
  }

  /** @param {UserSnippet} s */
  function remove(s) {
    removeSnippet(s.id)
    toast.message('Snippet deleted', {
      description: s.name,
      duration: 8000,
      action: { label: 'Undo', onClick: () => saveSnippet(s) },
    })
  }

  /** @param {string} body */
  async function copyBody(body) {
    try {
      await navigator.clipboard.writeText(body)
      toast.success('Copied the snippet')
    } catch {
      toast.error('Could not copy')
    }
  }

  async function exportAll() {
    if (!$userSnippets.length) return
    try {
      const path = await saveExportAs(snippetsToJson($userSnippets), 'stroke-snippets.json', { name: 'JSON', extensions: ['json'] })
      if (path) toast.success(`Exported ${$userSnippets.length} ${$userSnippets.length === 1 ? 'snippet' : 'snippets'}`, { description: path })
    } catch (e) {
      toast.error('Could not export', { description: String(e) })
    }
  }

  async function importFile() {
    try {
      const file = await openTextFileAs({ name: 'JSON', extensions: ['json'] })
      if (!file) return
      const parsed = snippetsFromJson(file.content)
      if ('error' in parsed) { toast.error('Could not import', { description: parsed.error }); return }
      if (!parsed.snippets.length) { toast.error('Nothing to import', { description: 'No snippet in that file has a name, a trigger and a body.' }); return }
      const { added, replaced } = importSnippets(parsed.snippets)
      const parts = [added && `${added} added`, replaced && `${replaced} updated`, parsed.skipped && `${parsed.skipped} skipped`].filter(Boolean)
      toast.success('Snippets imported', { description: parts.join(', ') })
    } catch (e) {
      toast.error('Could not import', { description: String(e) })
    }
  }

  /** The body as text and tab stops, so fields read as fields. @param {string} body */
  function bodyParts(body) {
    /** @type {{ text: string, stop?: string }[]} */
    const out = []
    const re = /\$\{(\d+)(?::([^}]*))?\}|\$(\d+)/g
    let last = 0
    for (let m = re.exec(body); m; m = re.exec(body)) {
      if (m.index > last) out.push({ text: body.slice(last, m.index) })
      out.push({ text: m[2] ?? '', stop: m[1] ?? m[3] })
      last = m.index + m[0].length
    }
    if (last < body.length) out.push({ text: body.slice(last) })
    return out
  }

  /** @param {string} id */
  function toggleScope(id) {
    if (!draft) return
    draft.scope = draft.scope.includes(id) ? draft.scope.filter((s) => s !== id) : [...draft.scope, id]
  }

  const field = 'h-8 w-full rounded-md border border-border bg-background px-2.5 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30'
  const iconAction = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring'
</script>

{#snippet bodyView(/** @type {string} */ body)}
  <pre class="overflow-x-auto rounded-md bg-muted/40 px-3 py-2 font-mono text-ui-xs leading-relaxed whitespace-pre text-foreground/90">{#each bodyParts(body) as p, i (i)}{#if p.stop}<span class="rounded-sm bg-primary/12 px-0.5 text-primary" title="Field {p.stop}: Tab moves here">{p.text || `$${p.stop}`}</span>{:else}{p.text}{/if}{/each}</pre>
{/snippet}

{#snippet scopeChips(/** @type {string[]} */ scope)}
  {#if scope.length}
    {#each scope as id (id)}
      <span class="rounded-full border border-border/70 px-1.5 py-px text-ui-3xs font-medium leading-4 text-muted-foreground">{scopeLabel(id)}</span>
    {/each}
  {/if}
{/snippet}

<div class="flex flex-col gap-5">
  <p class="text-ui-sm text-pretty text-muted-foreground">
    SQL you reuse, a word away. Type a snippet's trigger in the SQL editor and pick it from the list; Tab moves between its fields.
  </p>

  <div class="flex flex-wrap items-center gap-2">
    <!-- rank null: Settings' own search already answers `/`. -->
    <SearchField bind:value={query} placeholder="Search snippets" label="Search snippets" rank={null} class="min-w-40 flex-1" />
    <SelectMenu
      ariaLabel="Engine"
      triggerClass="h-8 w-40 shrink-0"
      contentClass="w-[var(--bits-popover-anchor-width)] min-w-[10rem]"
      align="end"
      value={scopeFilter}
      onValueChange={(v) => (scopeFilter = v)}
      items={[{ value: '', label: 'Every engine' }, ...SNIPPET_SCOPES.map((s) => ({ value: s.id, label: s.label }))]}
    />
  </div>

  {#if draft}
    <section bind:this={draftEl} class="flex flex-col gap-3 rounded-lg border border-ring/50 bg-card p-4" aria-label={draft.id ? 'Edit snippet' : 'New snippet'}>
      <div class="grid gap-3 sm:grid-cols-[1fr_10rem]">
        <label class="flex flex-col gap-1">
          <span class="text-ui-xs font-medium text-foreground">Name</span>
          <input class={field} placeholder="Recent orders" bind:value={draft.name} />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-ui-xs font-medium text-foreground">Trigger</span>
          <input class={cn(field, 'font-mono')} placeholder="recent" spellcheck="false" autocomplete="off" bind:value={draft.trigger} />
        </label>
      </div>
      <label class="flex flex-col gap-1">
        <span class="text-ui-xs font-medium text-foreground">Description <span class="font-normal text-muted-foreground">optional</span></span>
        <input class={field} placeholder="What it is for" bind:value={draft.description} />
      </label>
      <div class="flex flex-col gap-1">
        <span class="text-ui-xs font-medium text-foreground">SQL</span>
        <div class="flex h-36 overflow-hidden rounded-md border border-border bg-background">
          {#key draftKey}
            <CodeEditor value={draft.body} lang="sql" onchange={(/** @type {string} */ v) => { if (draft) draft.body = v }} ariaLabel="Snippet SQL" />
          {/key}
        </div>
        <p class="text-ui-2xs text-muted-foreground"><code class="font-mono">{'${1:table}'}</code> is a field Tab moves to, with "table" as its placeholder. The same number twice is one field typed in both places.</p>
      </div>
      <div class="flex flex-col gap-1.5">
        <span class="text-ui-xs font-medium text-foreground">Engines</span>
        <div class="flex flex-wrap gap-1.5" role="group" aria-label="Engines">
          <button type="button" class={cn('h-7 rounded-full border px-2.5 text-ui-xs transition-colors', draft.scope.length === 0 ? 'border-primary/60 bg-primary/10 text-foreground' : 'border-border text-muted-foreground hover:text-foreground')} aria-pressed={draft.scope.length === 0} onclick={() => { if (draft) draft.scope = [] }}>Every engine</button>
          {#each SNIPPET_SCOPES as s (s.id)}
            <button type="button" class={cn('h-7 rounded-full border px-2.5 text-ui-xs transition-colors', draft.scope.includes(s.id) ? 'border-primary/60 bg-primary/10 text-foreground' : 'border-border text-muted-foreground hover:text-foreground')} aria-pressed={draft.scope.includes(s.id)} onclick={() => toggleScope(s.id)}>{s.label}</button>
          {/each}
        </div>
      </div>
      <div class="flex flex-wrap items-center gap-2 pt-1">
        <p class="min-w-0 flex-1 text-ui-xs {draftError ? 'text-muted-foreground' : 'text-warning'}">
          {#if draftError}{draftError}{:else if triggerClash}"{triggerClash.name}" uses <span class="font-mono">{triggerClash.trigger}</span> too; both will be offered.{/if}
        </p>
        <Button variant="ghost" size="sm" onclick={() => (draft = null)}>Cancel</Button>
        <Button size="sm" disabled={!!draftError} onclick={commitDraft}>{draft.id ? 'Save' : 'Add snippet'}</Button>
      </div>
    </section>
  {/if}

  <section class="flex flex-col gap-2" aria-label="Your snippets">
    <div class="flex items-center gap-1">
      <h3 class="flex-1 text-ui-xs font-medium text-muted-foreground">Yours <span class="tabular-nums">{$userSnippets.length}</span></h3>
      <Button variant="ghost" size="sm" title="Import snippets from a JSON file" onclick={importFile}><Upload class="size-3.5 shrink-0" />Import</Button>
      <Button variant="ghost" size="sm" title="Save your snippets as a JSON file" disabled={!$userSnippets.length} onclick={exportAll}><Download class="size-3.5 shrink-0" />Export</Button>
      <Button size="sm" onclick={() => startDraft()}><Plus class="size-3.5 shrink-0" />New snippet</Button>
    </div>
    {#if !$userSnippets.length}
      <div class="rounded-lg border border-dashed border-border px-4 py-6 text-center">
        <p class="text-ui-sm text-foreground">No snippets of your own yet</p>
        <p class="mt-1 text-ui-xs text-muted-foreground">Add one, import a file, or duplicate a built-in one below to change it.</p>
      </div>
    {:else if !mine.length}
      <p class="px-1 py-2 text-ui-xs text-muted-foreground">None of yours match.</p>
    {:else}
      <ul class="flex flex-col gap-2">
        {#each mine as s (s.id)}
          <li class="group flex flex-col gap-2 rounded-lg border border-border/70 bg-card/40 p-3">
            <div class="flex items-start gap-2">
              <div class="min-w-0 flex-1">
                <p class="flex flex-wrap items-center gap-1.5">
                  <span class="text-ui-sm font-medium text-foreground">{s.name}</span>
                  <span class="rounded-md border border-primary/30 bg-primary/8 px-1.5 py-px font-mono text-ui-2xs text-primary">{s.trigger}</span>
                  {@render scopeChips(s.scope)}
                </p>
                {#if s.description}<p class="mt-0.5 text-ui-xs text-muted-foreground">{s.description}</p>{/if}
              </div>
              <div class="flex shrink-0 items-center">
                <button type="button" class={iconAction} title="Copy SQL" aria-label="Copy SQL of {s.name}" onclick={() => copyBody(s.body)}><Copy class="size-3.5 shrink-0" /></button>
                <button type="button" class={iconAction} title="Edit" aria-label="Edit {s.name}" onclick={() => startDraft(s)}><Pencil class="size-3.5 shrink-0" /></button>
                <button type="button" class={cn(iconAction, 'hover:bg-destructive/10 hover:text-destructive')} title="Delete" aria-label="Delete {s.name}" onclick={() => remove(s)}><Trash2 class="size-3.5 shrink-0" /></button>
              </div>
            </div>
            {@render bodyView(s.body)}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="flex flex-col gap-2" aria-label="Built-in snippets">
    <button type="button" class="flex items-center gap-1.5 self-start text-ui-xs font-medium text-muted-foreground hover:text-foreground" aria-expanded={builtinOpen || !!query.trim()} onclick={() => (builtinOpen = !builtinOpen)}>
      <ChevronRight class={cn('size-3.5 shrink-0 transition-transform duration-150', (builtinOpen || query.trim()) && 'rotate-90')} />
      Built-in <span class="tabular-nums">{builtins.length}</span>
    </button>
    {#if builtinOpen || query.trim()}
      <ul class="flex flex-col divide-y divide-border/50 rounded-lg border border-border/70">
        {#each builtins as s, i (i)}
          <li class="flex flex-col gap-1.5 px-3 py-2.5">
            <div class="flex items-center gap-2">
              <span class="min-w-0 truncate text-ui-sm text-foreground">{s.name}</span>
              <span class="shrink-0 rounded-md border border-border px-1.5 py-px font-mono text-ui-2xs text-muted-foreground">{s.trigger}</span>
              {@render scopeChips(s.scope.filter((id) => SNIPPET_SCOPES.some((x) => x.id === id)))}
              <span class="flex-1"></span>
              <Button variant="ghost" size="sm" class="h-7 shrink-0 text-muted-foreground" onclick={() => startDraft({ name: `${s.name} (copy)`, trigger: s.trigger, body: s.body, scope: s.scope.filter((id) => SNIPPET_SCOPES.some((x) => x.id === id)) })}>Duplicate</Button>
            </div>
            {@render bodyView(s.body)}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>
