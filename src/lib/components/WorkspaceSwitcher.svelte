<script>
  // The active workspace, and a menu to switch, create one, or open the
  // Workspaces manager. Each workspace shows its WorkspaceMark (a solid square
  // in its colour with its initial). Counts are its connections, from the full
  // saved list.
  import { workspaces, activeWorkspaceId, setActiveWorkspace, inWorkspace } from '$lib/stores/workspaces.js'
  import WorkspaceDialog from './WorkspaceDialog.svelte'
  import WorkspaceMark from './WorkspaceMark.svelte'
  import WorkspacesManager from './WorkspacesManager.svelte'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { cn } from '$lib/utils.js'
  import ChevronsUpDown from '@lucide/svelte/icons/chevrons-up-down'
  import Check from '@lucide/svelte/icons/check'
  import Plus from '@lucide/svelte/icons/plus'
  import Settings2 from '@lucide/svelte/icons/settings-2'

  /**
   * @type {{
   *   connections?: { workspaceId?: string | null }[],
   *   onchanged?: () => void,
   *   compact?: boolean,
   *   class?: string,
   * }}
   */
  let { connections = [], onchanged = () => {}, compact = false, class: className = '' } = $props()

  const active = $derived($workspaces.find((w) => w.id === $activeWorkspaceId) ?? $workspaces[0])
  const count = $derived(active ? inWorkspace(connections, active.id, $workspaces).length : 0)
  let dialogOpen = $state(false)
  let managerOpen = $state(false)
  let editing = $state(/** @type {import('$lib/stores/workspaces.js').Workspace | null} */ (null))

  /** @param {import('$lib/stores/workspaces.js').Workspace | null} w */
  function openDialog(w) {
    editing = w
    dialogOpen = true
  }

</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger
    class={cn(
      cn('group flex min-w-0 items-center rounded-md text-left', !compact && 'hover:bg-accent/60 data-[state=open]:bg-accent/60'),
      compact ? 'size-[calc(28px*var(--app-scale))] justify-center rounded-md transition-[background-color,scale] duration-150 ease-out hover:bg-accent active:scale-[0.94] data-[state=open]:bg-accent' : 'h-8 gap-2 px-1.5',
      className,
    )}
    title={`Workspace: ${active?.name ?? ""}`}
  >
    <!-- The workspace's mark (its colour and initial), here and in the menu, so
         the same tile means the same workspace everywhere. -->
    <WorkspaceMark workspace={active} size={compact ? 'chrome' : 'sm'} />
    {#if !compact}<span class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">{active?.name}</span>{/if}
    {#if !compact}<span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground">{count}</span>{/if}
    {#if !compact}<ChevronsUpDown class="size-3.5 shrink-0 text-muted-foreground/70 group-hover:text-muted-foreground" />{/if}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content align="start" class="min-w-60">
    {#each $workspaces as w (w.id)}
      {@const on = w.id === $activeWorkspaceId}
      <DropdownMenu.Item onSelect={() => { setActiveWorkspace(w.id); onchanged() }} class="gap-2">
        <WorkspaceMark workspace={w} size="xs" />
        <span data-slot="menu-label" class={cn('min-w-0 flex-1 truncate', on && 'text-foreground')}>{w.name}</span>
        <span data-slot="command-trailing" class="ml-auto flex shrink-0 items-center gap-1.5 pl-3">
          <span class="text-ui-2xs tabular-nums text-muted-foreground">{inWorkspace(connections, w.id, $workspaces).length}</span>
          <Check class={cn('size-3.5 shrink-0', on ? 'text-foreground' : 'invisible')} />
        </span>
      </DropdownMenu.Item>
    {/each}
    <DropdownMenu.Separator />
    <DropdownMenu.Item onSelect={() => openDialog(null)} class="gap-2">
      <span class="flex size-4 shrink-0 items-center justify-center text-muted-foreground"><Plus class="size-3.5 shrink-0" /></span>
      <span data-slot="menu-label" class="min-w-0 flex-1 truncate">New workspace</span>
    </DropdownMenu.Item>
    <DropdownMenu.Item onSelect={() => (managerOpen = true)} class="gap-2">
      <span class="flex size-4 shrink-0 items-center justify-center text-muted-foreground"><Settings2 class="size-3.5 shrink-0" /></span>
      <span data-slot="menu-label" class="min-w-0 flex-1 truncate">Manage workspaces</span>
    </DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>

<WorkspaceDialog bind:open={dialogOpen} workspace={editing} {onchanged} />
<WorkspacesManager bind:open={managerOpen} {connections} {onchanged} />
