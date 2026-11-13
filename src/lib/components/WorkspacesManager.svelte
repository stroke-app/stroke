<script>
  // Manage workspaces: search, create, and per row switch / edit / delete.
  // Rows show the workspace's mark, name and how many connections it holds.
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import WorkspaceMark from './WorkspaceMark.svelte'
  import WorkspaceDialog from './WorkspaceDialog.svelte'
  import { workspaces, activeWorkspaceId, setActiveWorkspace, inWorkspace, deleteWorkspace, DEFAULT_WORKSPACE_ID } from '$lib/stores/workspaces.js'
  import { releaseWorkspace } from '$lib/stores/connections.js'
  import { cn } from '$lib/utils.js'
  import Search from '@lucide/svelte/icons/search'
  import Plus from '@lucide/svelte/icons/plus'
  import EllipsisVertical from '@lucide/svelte/icons/ellipsis-vertical'
  import Pencil from '@lucide/svelte/icons/pencil'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import Check from '@lucide/svelte/icons/check'

  /**
   * @type {{ open?: boolean, connections?: { workspaceId?: string | null }[], onchanged?: () => void }}
   */
  let { open = $bindable(false), connections = [], onchanged = () => {} } = $props()

  let query = $state('')
  let editOpen = $state(false)
  let editing = $state(/** @type {import('$lib/stores/workspaces.js').Workspace | null} */ (null))
  let confirmDeleteId = $state(/** @type {string | null} */ (null))

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return q ? $workspaces.filter((w) => w.name.toLowerCase().includes(q)) : $workspaces
  })

  /** @param {import('$lib/stores/workspaces.js').Workspace | null} w */
  function edit(w) {
    editing = w
    editOpen = true
  }

  /** @param {string} id */
  function remove(id) {
    if (confirmDeleteId !== id) {
      confirmDeleteId = id
      return
    }
    deleteWorkspace(id, releaseWorkspace)
    confirmDeleteId = null
    onchanged()
  }

  /** @param {number} n */
  const countLabel = (n) => (n === 0 ? 'No connections' : n === 1 ? '1 connection' : `${n} connections`)
</script>

<Dialog.Root bind:open>
  <Dialog.Content showCloseButton={false} class="w-[min(34rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-2xl p-0 sm:max-w-none">
    <div class="flex flex-col gap-3 p-4 pb-2">
      <div class="flex items-center justify-between">
        <Dialog.Title class="text-ui-sm font-semibold text-foreground">Workspaces</Dialog.Title>
        <Dialog.Description class="sr-only">Create, rename, recolour or delete workspaces</Dialog.Description>
      </div>
      <div class="flex items-center gap-2">
        <div class="relative flex h-8 min-w-0 flex-1 items-center">
          <Search class="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground" />
          <input
            bind:value={query}
            placeholder="Search…"
            aria-label="Search workspaces"
            spellcheck="false"
            autocomplete="off"
            class="field-surface h-8 w-full rounded-lg bg-input/30 pl-8 pr-2 text-ui-sm text-foreground placeholder:text-muted-foreground"
          />
        </div>
        <Button size="sm" class="h-8 rounded-lg" onclick={() => edit(null)}><Plus /> Create</Button>
      </div>
    </div>

    <ul class="app-scroll flex max-h-[min(24rem,60vh)] flex-col gap-0.5 overflow-y-auto px-2 pb-3" role="list">
      {#each shown as w (w.id)}
        {@const n = inWorkspace(connections, w.id, $workspaces).length}
        {@const active = w.id === $activeWorkspaceId}
        <li class={cn('group flex items-center gap-3 rounded-lg px-2 py-2 transition-colors', active ? 'bg-accent/70' : 'hover:bg-accent/40')}>
          <button type="button" class="flex min-w-0 flex-1 items-center gap-3 text-left" onclick={() => { setActiveWorkspace(w.id); onchanged() }} title={`Switch to ${w.name}`}>
            <WorkspaceMark workspace={w} size="lg" />
            <span class="flex min-w-0 flex-col">
              <span class="truncate text-ui-sm font-medium text-foreground">{w.name}</span>
              <span class="truncate text-ui-xs text-muted-foreground">{countLabel(n)}{w.id === DEFAULT_WORKSPACE_ID ? ' · Default' : ''}</span>
            </span>
          </button>
          {#if active}<Check class="size-4 shrink-0 text-foreground" aria-label="Active" />{/if}
          {#if confirmDeleteId === w.id}
            <Button size="sm" variant="destructive" class="rounded-lg" onclick={() => remove(w.id)} onblur={() => (confirmDeleteId = null)}>Delete; connections move to Personal</Button>
          {:else}
            <DropdownMenu.Root>
              <DropdownMenu.Trigger class="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground opacity-0 transition-opacity hover:bg-accent hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:bg-accent data-[state=open]:opacity-100" aria-label={`Actions for ${w.name}`}>
                <EllipsisVertical class="size-4 shrink-0" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end" class="min-w-40">
                <DropdownMenu.Item onSelect={() => edit(w)}><Pencil /> Edit</DropdownMenu.Item>
                {#if w.id !== DEFAULT_WORKSPACE_ID}
                  <DropdownMenu.Item onSelect={() => remove(w.id)} class="text-destructive data-highlighted:text-destructive"><Trash2 /> Delete</DropdownMenu.Item>
                {/if}
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          {/if}
        </li>
      {:else}
        <li class="px-2 py-6 text-center text-ui-xs text-muted-foreground">No workspace matches “{query}”.</li>
      {/each}
    </ul>
  </Dialog.Content>
</Dialog.Root>

<WorkspaceDialog bind:open={editOpen} workspace={editing} {onchanged} />
