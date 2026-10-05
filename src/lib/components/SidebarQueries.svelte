<script>
  /**
   * The sidebar's Queries tab: the connection's saved queries as a tree, with
   * folders. The same store as Saved in the SQL console's results pane, so the
   * two always list the same queries; this one files them.
   */
  import { tick, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import { buildQueryTree, copyName, uniqueFolderName, planFolderDelete } from '$lib/query-folders.js'
  import { loadQueryFolders, saveQueryFolders } from '$lib/stores/query-folders.js'
  import {
    createSavedQuery, updateSavedQuery, deleteSavedQuery, restoreSavedQuery,
  } from '$lib/stores/query-history.js'

  /** @typedef {import('$lib/stores/query-history.js').SavedQuery} SavedQuery */
  /** @typedef {import('$lib/query-folders.js').QueryFolder} QueryFolder */

  let {
    connectionId = '',
    /** @type {SavedQuery[]} */
    queries = [],
    /** The saved query whose editor tab is in front. */
    activeId = /** @type {string | null} */ (null),
    /** Open it in an editor tab, or bring its tab forward. @type {(q: SavedQuery) => void} */
    onopen = () => {},
    /** Open it and run it. @type {(q: SavedQuery) => void} */
    onrun = () => {},
    /** A new editor tab tied to a new saved query in this folder (null: the root). @type {(folderId: string | null) => void} */
    onnew = () => {},
    /** Reload the saved queries after a change. */
    onrefresh = async () => {},
  } = $props()

  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform)
  const modKey = isMac ? '⌘' : 'Ctrl'

  // ── Folders ───────────────────────────────────────────────────────────────
  /** @type {QueryFolder[]} */
  let folders = $state([])
  /** Folded folders, by id; kept per connection. @type {Set<string>} */
  let collapsed = $state(new Set())
  const collapsedKey = (/** @type {string} */ cid) => `stroke:query-folders-collapsed:${cid || '_default'}`
  $effect(() => {
    const cid = connectionId
    untrack(() => {
      folders = loadQueryFolders(cid)
      try { collapsed = new Set(JSON.parse(localStorage.getItem(collapsedKey(cid)) ?? '[]')) } catch { collapsed = new Set() }
    })
  })
  /** @param {QueryFolder[]} next */
  function setFolders(next) {
    folders = next
    saveQueryFolders(connectionId, next)
  }
  /** @param {string} id */
  function toggleFolder(id) {
    const next = new Set(collapsed)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    collapsed = next
    try { localStorage.setItem(collapsedKey(connectionId), JSON.stringify([...next])) } catch { /* only the fold is lost */ }
  }

  let filter = $state('')
  const tree = $derived(buildQueryTree(queries, folders, filter))
  const isEmpty = $derived(queries.length === 0 && folders.length === 0)

  // ── Inline rename ─────────────────────────────────────────────────────────
  /** @type {{ kind: 'query' | 'folder', id: string } | null} */
  let renaming = $state(null)
  let renameValue = $state('')

  /** @param {'query' | 'folder'} kind @param {string} id @param {string} name */
  async function startRename(kind, id, name) {
    renaming = { kind, id }
    renameValue = name
    await tick()
    const el = /** @type {HTMLInputElement | null} */ (document.querySelector('[data-query-rename]'))
    el?.focus()
    el?.select()
  }

  async function commitRename() {
    const r = renaming
    if (!r) return
    renaming = null
    const name = renameValue.trim()
    if (!name) return
    if (r.kind === 'folder') {
      setFolders(folders.map((f) => (f.id === r.id ? { ...f, name } : f)))
      return
    }
    const current = queries.find((q) => q.id === r.id)
    if (!current || current.name === name) return
    await updateSavedQuery(r.id, { name })
    await onrefresh()
  }

  /** @param {KeyboardEvent} e */
  function onRenameKey(e) {
    if (e.key === 'Enter') { e.preventDefault(); void commitRename() }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); renaming = null }
  }

  // ── Query actions ─────────────────────────────────────────────────────────
  /**
   * A click opens the query at once; a double click runs it. The first click
   * of the pair has already opened the tab, so the run lands in that tab
   * rather than a second one. Rename is F2 or the context menu.
   * @param {MouseEvent} e @param {SavedQuery} q
   */
  function onRowClick(e, q) {
    if (dragJustEnded || e.detail > 1) return
    onopen(q)
  }
  /** @param {SavedQuery} q */
  function onRowDblClick(q) {
    if (dragJustEnded || !q.sql.trim()) return
    onrun(q)
  }

  /** @param {SavedQuery} q */
  async function duplicate(q) {
    await createSavedQuery(connectionId, copyName(q.name, queries), q.sql, { folderId: q.folderId ?? null, allowEmpty: true })
    await onrefresh()
  }

  /** @param {SavedQuery} q @param {string | null} folderId */
  async function moveTo(q, folderId) {
    if ((q.folderId ?? null) === folderId) return
    await updateSavedQuery(q.id, { folderId })
    await onrefresh()
  }

  /** @param {SavedQuery} q */
  async function copySql(q) {
    try {
      await navigator.clipboard.writeText(q.sql)
      toast.success('SQL copied', { description: q.name })
    } catch { /* clipboard unavailable */ }
  }

  /** Delete now, Undo in the toast. @param {SavedQuery} q */
  async function removeQuery(q) {
    const entry = $state.snapshot(q)
    await deleteSavedQuery(q.id)
    await onrefresh()
    toast.message('Saved query deleted', {
      description: q.name,
      action: { label: 'Undo', onClick: () => void restoreSavedQuery(entry).then(() => onrefresh()) },
    })
  }

  // ── Folder actions ────────────────────────────────────────────────────────
  async function newFolder() {
    const folder = { id: crypto.randomUUID(), name: uniqueFolderName(folders) }
    setFolders([...folders, folder])
    filter = ''
    await startRename('folder', folder.id, folder.name)
  }

  /** The folder waiting on the delete dialog. @type {QueryFolder | null} */
  let deleting = $state(null)
  const deletingCount = $derived(deleting ? queries.filter((q) => q.folderId === deleting?.id).length : 0)

  /** @param {QueryFolder} folder */
  function askDeleteFolder(folder) {
    if (queries.some((q) => q.folderId === folder.id)) deleting = folder
    else void deleteFolder(folder, false)
  }

  /**
   * Its queries go to the root unless `withQueries`. Undo puts the folder, and
   * whatever left it, back.
   * @param {QueryFolder} folder @param {boolean} withQueries
   */
  async function deleteFolder(folder, withQueries) {
    deleting = null
    const before = $state.snapshot(folders)
    const plan = planFolderDelete(queries, folder.id, withQueries)
    const removed = queries.filter((q) => plan.remove.includes(q.id)).map((q) => $state.snapshot(q))
    setFolders(folders.filter((f) => f.id !== folder.id))
    await Promise.all([
      ...plan.toRoot.map((id) => updateSavedQuery(id, { folderId: null })),
      ...plan.remove.map((id) => deleteSavedQuery(id)),
    ])
    await onrefresh()
    toast.message(`Folder "${folder.name}" deleted`, {
      description: plan.toRoot.length
        ? `${plan.toRoot.length} ${plan.toRoot.length === 1 ? 'query' : 'queries'} moved out of it.`
        : plan.remove.length ? `${plan.remove.length} ${plan.remove.length === 1 ? 'query' : 'queries'} deleted with it.` : undefined,
      action: {
        label: 'Undo',
        onClick: () => void (async () => {
          setFolders(before)
          await Promise.all([
            ...plan.toRoot.map((id) => updateSavedQuery(id, { folderId: folder.id })),
            ...removed.map((q) => restoreSavedQuery(q)),
          ])
          await onrefresh()
        })(),
      },
    })
  }

  // ── Drag a query onto a folder ────────────────────────────────────────────
  // Pointer events, not HTML drag and drop: the tab bar drags the same way, and
  // native drag inside the webview is at the mercy of the window's file drop.
  /** @type {{ id: string, x: number, y: number, active: boolean } | null} */
  let drag = $state(null)
  /** The folder under the pointer: an id, '' for the root, null for nowhere. */
  let dropTarget = $state(/** @type {string | null} */ (null))
  let dragJustEnded = false

  /** @param {PointerEvent} e @param {SavedQuery} q */
  function onRowPointerDown(e, q) {
    if (e.button !== 0 || renaming) return
    drag = { id: q.id, x: e.clientX, y: e.clientY, active: false }
    window.addEventListener('pointermove', onDragMove)
    window.addEventListener('pointerup', onDragEnd, { once: true })
  }
  /** @param {PointerEvent} e */
  function onDragMove(e) {
    if (!drag) return
    if (!drag.active && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 5) return
    drag.active = true
    const zone = /** @type {HTMLElement | null} */ (document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-drop-folder]') ?? null)
    dropTarget = zone ? zone.dataset.dropFolder ?? null : null
  }
  async function onDragEnd() {
    window.removeEventListener('pointermove', onDragMove)
    const d = drag
    const target = dropTarget
    drag = null
    dropTarget = null
    if (!d?.active) return
    // The click that ends a drag is not an open.
    dragJustEnded = true
    setTimeout(() => { dragJustEnded = false }, 0)
    const q = queries.find((x) => x.id === d.id)
    if (q && target !== null) await moveTo(q, target || null)
  }

  // ── One context menu for the tree ─────────────────────────────────────────
  /** @type {{ kind: 'query', query: SavedQuery } | { kind: 'folder', folder: QueryFolder } | null} */
  let menu = $state(null)

  /** F2 renames, Delete deletes, from the focused row. @param {KeyboardEvent} e @param {SavedQuery} q */
  function onRowKey(e, q) {
    if (e.key === 'F2') { e.preventDefault(); void startRename('query', q.id, q.name) }
    else if (e.key === 'Delete') { e.preventDefault(); void removeQuery(q) }
  }
</script>

{#snippet queryRow(/** @type {SavedQuery} */ q, /** @type {boolean} */ nested)}
  {#if renaming?.kind === 'query' && renaming.id === q.id}
    <div class={cn('flex h-7 items-center gap-2 px-2', nested && 'pl-7')}>
      <Icon name="file-code" class="size-3.5 shrink-0 text-muted-foreground" />
      <input
        data-query-rename
        bind:value={renameValue}
        onkeydown={onRenameKey}
        onblur={() => void commitRename()}
        aria-label="Name of the saved query"
        class="field-surface h-6 min-w-0 flex-1 bg-background/60 px-1.5 text-ui-sm outline-none"
      />
    </div>
  {:else}
    <button
      type="button"
      data-query-row={q.id}
      class={cn(
        'flex h-7 w-full items-center gap-2 rounded-md px-2 text-left transition-colors',
        nested && 'pl-7',
        activeId === q.id ? 'bg-sidebar-accent text-sidebar-accent-foreground' : 'text-foreground/70 hover:bg-sidebar-accent/50 hover:text-foreground',
        drag?.active && drag.id === q.id && 'opacity-50',
      )}
      title={q.sql ? `${q.name} · double-click to run\n\n${q.sql.slice(0, 400)}` : q.name}
      onclick={(e) => onRowClick(e, q)}
      ondblclick={() => onRowDblClick(q)}
      onkeydown={(e) => onRowKey(e, q)}
      onpointerdown={(e) => onRowPointerDown(e, q)}
      oncontextmenu={() => (menu = { kind: 'query', query: q })}
    >
      <Icon name="file-code" class={cn('size-3.5 shrink-0', activeId === q.id ? 'text-primary' : 'text-muted-foreground')} />
      <span class="min-w-0 flex-1 truncate text-ui-sm leading-4">{q.name}</span>
    </button>
  {/if}
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  <!-- Filter, then New folder and New query. -->
  <div class="flex h-9 shrink-0 items-center gap-1 border-b border-sidebar-border px-2">
    <div class="relative min-w-0 flex-1">
      <Icon name="search" class="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        type="text"
        bind:value={filter}
        onkeydown={(e) => { if (e.key === 'Escape' && filter) { e.preventDefault(); e.stopPropagation(); filter = '' } }}
        placeholder="Filter queries…"
        aria-label="Filter saved queries"
        class="field-surface h-7 w-full min-w-0 bg-background/40 pl-7 pr-2 text-ui-sm text-foreground shadow-none outline-none placeholder:text-muted-foreground"
      />
    </div>
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
      title="New folder"
      aria-label="New folder"
      disabled={!connectionId}
      onclick={() => void newFolder()}
    ><Icon name="folder-plus" class="size-3.5" /></button>
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
      title="New query"
      aria-label="New query"
      disabled={!connectionId}
      onclick={() => onnew(null)}
    ><Icon name="plus" class="size-3.5" /></button>
  </div>

  {#if isEmpty}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
      <Icon name="file-code" class="size-5 text-muted-foreground" />
      <p class="text-ui-sm text-foreground">No saved queries</p>
      <p class="text-ui-xs text-muted-foreground">{modKey}S in a query editor saves its query here.</p>
      <Button variant="outline" size="sm" class="mt-1" disabled={!connectionId} onclick={() => onnew(null)}>
        <Icon name="plus" class="size-3.5" />New query
      </Button>
    </div>
  {:else}
    <ContextMenu.Root onOpenChange={(open) => { if (!open) menu = null }}>
      <ContextMenu.Trigger class="flex min-h-0 flex-1 flex-col">
        <div
          class={cn('app-scroll min-h-0 flex-1 overflow-y-auto px-1.5 py-1', drag?.active && dropTarget === '' && 'bg-primary/5')}
          data-drop-folder=""
          role="group"
          aria-label="Saved queries"
        >
          {#each tree.folders as group (group.folder.id)}
            {@const f = group.folder}
            {@const open = !collapsed.has(f.id) || !!filter.trim()}
            <div
              role="group"
              aria-label={f.name}
              data-drop-folder={f.id}
              class={cn('rounded-md', drag?.active && dropTarget === f.id && 'bg-primary/10 ring-1 ring-primary/25')}
            >
              {#if renaming?.kind === 'folder' && renaming.id === f.id}
                <div class="flex h-7 items-center gap-2 px-2">
                  <Icon name="folder" class="size-3.5 shrink-0 text-muted-foreground" />
                  <input
                    data-query-rename
                    bind:value={renameValue}
                    onkeydown={onRenameKey}
                    onblur={() => void commitRename()}
                    aria-label="Name of the folder"
                    class="field-surface h-6 min-w-0 flex-1 bg-background/60 px-1.5 text-ui-sm outline-none"
                  />
                </div>
              {:else}
                <div class="group/folder flex h-7 items-center rounded-md text-foreground/80 hover:bg-sidebar-accent/50">
                  <button
                    type="button"
                    class="flex h-full min-w-0 flex-1 items-center gap-1.5 pl-1 text-left"
                    aria-expanded={open}
                    onclick={() => toggleFolder(f.id)}
                    ondblclick={() => void startRename('folder', f.id, f.name)}
                    onkeydown={(e) => {
                      if (e.key === 'F2') { e.preventDefault(); void startRename('folder', f.id, f.name) }
                      else if (e.key === 'Delete') { e.preventDefault(); askDeleteFolder(f) }
                    }}
                    oncontextmenu={() => (menu = { kind: 'folder', folder: f })}
                    title="{f.name} · double-click or F2 to rename"
                  >
                    <Icon name="chevron-right" class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-150', open && 'rotate-90')} />
                    <Icon name={open ? 'folder-open' : 'folder'} class="size-3.5 shrink-0 text-muted-foreground" />
                    <span class="min-w-0 flex-1 truncate text-ui-sm leading-4">{f.name}</span>
                    <span class="shrink-0 pr-1 font-mono text-ui-2xs tabular-nums text-muted-foreground">{group.queries.length}</span>
                  </button>
                  <button
                    type="button"
                    tabindex="-1"
                    class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[opacity,background-color,color] hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover/folder:opacity-100"
                    title="New query in {f.name}"
                    aria-label="New query in {f.name}"
                    onclick={() => onnew(f.id)}
                  ><Icon name="plus" class="size-3.5" /></button>
                </div>
              {/if}
              {#if open}
                {#each group.queries as q (q.id)}
                  {@render queryRow(q, true)}
                {:else}
                  <p class="h-7 pl-7 text-ui-xs leading-7 text-muted-foreground">Empty. Drag a query here.</p>
                {/each}
              {/if}
            </div>
          {/each}
          {#each tree.root as q (q.id)}
            {@render queryRow(q, false)}
          {/each}
          {#if filter.trim() && !tree.root.length && !tree.folders.length}
            <p class="px-2 py-3 text-ui-xs text-muted-foreground">No query matches "{filter.trim()}".</p>
          {/if}
        </div>
      </ContextMenu.Trigger>
      <ContextMenu.Content class="min-w-52">
        {#if menu?.kind === 'query'}
          {@const q = menu.query}
          <ContextMenu.Item onSelect={() => onopen(q)}><Icon name="external-link" />Open</ContextMenu.Item>
          <ContextMenu.Item onSelect={() => onrun(q)} disabled={!q.sql.trim()}><Icon name="play" />Run</ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => void startRename('query', q.id, q.name)}>
            <Icon name="pencil" />Rename<ContextMenu.Shortcut>F2</ContextMenu.Shortcut>
          </ContextMenu.Item>
          <ContextMenu.Item onSelect={() => void duplicate(q)}><Icon name="copy-plus" />Duplicate</ContextMenu.Item>
          <ContextMenu.Sub>
            <ContextMenu.SubTrigger><Icon name="folder" />Move to folder</ContextMenu.SubTrigger>
            <ContextMenu.SubContent class="min-w-44">
              <ContextMenu.Item disabled={!q.folderId || !folders.some((f) => f.id === q.folderId)} onSelect={() => void moveTo(q, null)}>
                <Icon name="file-code" />No folder
              </ContextMenu.Item>
              {#if folders.length}<ContextMenu.Separator />{/if}
              {#each folders as f (f.id)}
                <ContextMenu.Item disabled={q.folderId === f.id} onSelect={() => void moveTo(q, f.id)}>
                  <Icon name="folder" /><span data-slot="menu-label">{f.name}</span>
                </ContextMenu.Item>
              {/each}
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => void (async () => {
                const folder = { id: crypto.randomUUID(), name: uniqueFolderName(folders) }
                setFolders([...folders, folder])
                await moveTo(q, folder.id)
                await startRename('folder', folder.id, folder.name)
              })()}><Icon name="folder-plus" />New folder</ContextMenu.Item>
            </ContextMenu.SubContent>
          </ContextMenu.Sub>
          <ContextMenu.Item onSelect={() => void copySql(q)} disabled={!q.sql.trim()}><Icon name="copy" />Copy SQL</ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item variant="destructive" onSelect={() => void removeQuery(q)}>
            <Icon name="trash-2" />Delete<ContextMenu.Shortcut>Del</ContextMenu.Shortcut>
          </ContextMenu.Item>
        {:else if menu?.kind === 'folder'}
          {@const f = menu.folder}
          <ContextMenu.Item onSelect={() => onnew(f.id)}><Icon name="plus" />New query here</ContextMenu.Item>
          <ContextMenu.Item onSelect={() => void startRename('folder', f.id, f.name)}>
            <Icon name="pencil" />Rename<ContextMenu.Shortcut>F2</ContextMenu.Shortcut>
          </ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item variant="destructive" onSelect={() => askDeleteFolder(f)}><Icon name="trash-2" />Delete folder</ContextMenu.Item>
        {:else}
          <ContextMenu.Item onSelect={() => onnew(null)}><Icon name="plus" />New query</ContextMenu.Item>
          <ContextMenu.Item onSelect={() => void newFolder()}><Icon name="folder-plus" />New folder</ContextMenu.Item>
        {/if}
      </ContextMenu.Content>
    </ContextMenu.Root>
  {/if}
</div>

<!-- A folder with queries in it: they move out by default, and go with it
     only when that is the button pressed. -->
<Dialog.Root open={!!deleting} onOpenChange={(open) => { if (!open) deleting = null }}>
  <Dialog.Content class="max-w-md gap-4">
    <Dialog.Header>
      <Dialog.Title class="text-ui-sm font-semibold">Delete folder "{deleting?.name}"?</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        It holds {deletingCount} saved {deletingCount === 1 ? 'query' : 'queries'}. They can move out of it, or go with it.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer class="gap-2 sm:justify-end">
      <Button type="button" variant="ghost" size="sm" onclick={() => (deleting = null)}>Cancel</Button>
      <Button type="button" variant="outline" size="sm" class="text-destructive hover:text-destructive" onclick={() => deleting && void deleteFolder(deleting, true)}>
        Delete with {deletingCount === 1 ? 'its query' : `its ${deletingCount} queries`}
      </Button>
      <Button type="button" size="sm" onclick={() => deleting && void deleteFolder(deleting, false)}>Keep the queries</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
