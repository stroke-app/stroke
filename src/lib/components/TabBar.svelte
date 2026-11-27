<script>
  import { titleBarDoubleClick } from '$lib/titlebar-double-click.js'
  import { onMount, tick } from 'svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { t } from '$lib/i18n.js'
  import { tabDisplayTitle } from '$lib/studio-tabs.js'
  import { withShortcut } from '$lib/stores/keybindings.js'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import SearchableMenu from './SearchableMenu.svelte'

  let isTauri = $state(false)

  onMount(() => {
    isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
  })

  /** @param {MouseEvent} e */
  function handleDragAreaDblClick(e) {
    if (!isTauri) return
    // Direct double-clicks on the header are already handled by Tauri's
    // injected drag-region script (data-tauri-drag-region) - toggling here
    // too made the window maximize and instantly restore. This handler only
    // covers double-clicks on non-interactive children the built-in ignores.
    if (e.target === e.currentTarget) return
    if (/** @type {Element} */ (e.target).hasAttribute?.('data-tauri-drag-region')) return
    if (/** @type {Element} */ (e.target).closest('button')) return
    void titleBarDoubleClick()
  }

  /** @typedef {import('$lib/studio-tabs.js').StudioTab} StudioTab */

  let {
    tabs = [],
    activeTabId = null,
    onselect = () => {},
    onclose = () => {},
    oncloseothers = /** @param {string} _id */ (_id) => {},
    oncloseall = () => {},
    /** Close a batch of tabs (Close Tabs to Left/Right). anchorId is the tab whose menu was used. */
    onclosemany = /** @type {(ids: string[], anchorId: string) => void} */ (() => {}),
    onduplicate = /** @param {string} _id */ (_id) => {},
    /** Reset a table tab's view state - search, filters, sort, hidden columns, view mode. */
    onresettable = /** @param {string} _id */ (_id) => {},
    onreopenclosed = () => {},
    /** Back and forward through the tabs visited, browser-style. Absent: no buttons. */
    onback = /** @type {(() => void) | null} */ (null),
    onforward = /** @type {(() => void) | null} */ (null),
    canback = false,
    canforward = false,
    /** Open a new tab, from the + after the last one. Absent: no button. */
    onnewtab = /** @type {(() => void) | null} */ (null),
    /** Open a fresh query editor - the `+` at the end of the strip. */
    onnewsql = () => {},
    /** Whether the closed-tab stack has anything to reopen. */
    canreopenclosed = false,
    onpintoggle = /** @param {string} _id */ (_id) => {},
    /** Pointer-based tab drag → split panes. Start/move/end drive the drop hints. */
    ondragtabstart = /** @param {string} _id */ (_id) => {},
    ondragtabmove = /** @type {(clientX: number, clientY: number) => void} */ (() => {}),
    ondragtabend = () => {},
    /** Rename a query editor tab. Absent: no Rename. @type {((id: string, title: string) => void) | null} */
    onrename = null,
    /** Bring the sidebar back. Passed only while it is hidden. @type {(() => void) | null} */
    onshowsidebar = null,
  } = $props()

  // ── Rename a query editor tab in place ─────────────────────────────────────
  // Every editor tab read "Query Editor", "Query Editor 2"...; a name says
  // which buffer is which. Double-click the tab or use its menu; Enter or
  // leaving the field keeps it, Escape puts the old name back.
  let renamingId = $state(/** @type {string | null} */ (null))
  let renameValue = $state('')
  /** @param {any} tab */
  const canRename = (tab) => !!onrename && tab.kind === 'sql'
  /** @param {any} tab */
  function startRename(tab) {
    if (!canRename(tab)) return
    renameValue = tabDisplayTitle(tab)
    renamingId = tab.id
  }
  /** @param {boolean} [refocus] back to the tab, after Enter or Escape */
  function commitRename(refocus = false, keep = true) {
    const id = renamingId
    const title = renameValue.trim()
    renamingId = null
    // Unchanged: nothing to rename (the field starts on the shown title).
    const was = id ? tabs.find((t) => t.id === id) : null
    if (id && title && keep && (!was || title !== tabDisplayTitle(was))) onrename?.(id, title)
    if (refocus && id) {
      void tick().then(() => /** @type {HTMLElement | null} */ (document.querySelector(`[data-tab-id="${CSS.escape(id)}"] [role="tab"]`))?.focus())
    }
  }
  /** @param {HTMLInputElement} el */
  function focusSelect(el) {
    // Again a moment later: the selection made on mount was collapsed to the
    // end by the tab re-rendering after the click, so typing appended to the
    // old name.
    el.focus()
    el.select()
    setTimeout(() => { if (document.activeElement === el) el.select() }, 80)
  }

  /** Titles that tabs from different databases share (two "orders" tables):
   *  those tabs name their database after the title. */
  const clashing = $derived.by(() => {
    /** @type {Map<string, Set<string>>} */
    const by = new Map()
    for (const t of tabs) {
      if (!t.connLabel) continue
      const title = tabDisplayTitle(t)
      by.set(title, (by.get(title) ?? new Set()).add(t.connLabel))
    }
    return new Set([...by].filter(([, dbs]) => dbs.size > 1).map(([title]) => title))
  })

  // ── Pointer-based tab drag (HTML5 DnD is unreliable in Tauri's WebKit) ──────
  /** @type {{ id: string, startX: number, startY: number, active: boolean } | null} */
  let _tabDrag = null
  /** Set true when a drag occurred, so the ensuing click doesn't also select the tab. */
  let _suppressTabClick = false

  /** @param {PointerEvent} e @param {string} id */
  function tabPointerDown(e, id) {
    if (e.button !== 0) return
    _tabDrag = { id, startX: e.clientX, startY: e.clientY, active: false }
    window.addEventListener('pointermove', tabPointerMove)
    window.addEventListener('pointerup', tabPointerUp)
    window.addEventListener('pointercancel', tabPointerUp)
  }
  /** @param {PointerEvent} e */
  function tabPointerMove(e) {
    if (!_tabDrag) return
    if (!_tabDrag.active) {
      if (Math.abs(e.clientX - _tabDrag.startX) <= 5 && Math.abs(e.clientY - _tabDrag.startY) <= 5) return
      _tabDrag.active = true
      ondragtabstart(_tabDrag.id)
    }
    ondragtabmove(e.clientX, e.clientY)
  }
  function tabPointerUp() {
    window.removeEventListener('pointermove', tabPointerMove)
    window.removeEventListener('pointerup', tabPointerUp)
    window.removeEventListener('pointercancel', tabPointerUp)
    if (_tabDrag?.active) { _suppressTabClick = true; ondragtabend() }
    _tabDrag = null
  }

  /** @type {HTMLElement | null} */
  let scrollEl = $state(null)

  // Which edges have hidden tabs: those edges fade out instead of cutting a
  // title in half, and only while there is actually more to scroll to.
  const navBtn = 'inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
  let fadeLeft = $state(false)
  let fadeRight = $state(false)
  function updateFades() {
    const el = scrollEl
    if (!el) return
    fadeLeft = el.scrollLeft > 1
    fadeRight = el.scrollLeft + el.clientWidth < el.scrollWidth - 1
  }
  $effect(() => {
    const el = scrollEl
    if (!el) return
    const _n = tabs.length
    // In the frame, not now: read during the render it forced an early layout
    // of the whole page (see Sidebar's scheduleMeasureTables).
    const frame = requestAnimationFrame(updateFades)
    const ro = new ResizeObserver(updateFades)
    ro.observe(el)
    return () => { cancelAnimationFrame(frame); ro.disconnect() }
  })

  /** A plain mouse wheel scrolls the strip sideways. @param {WheelEvent} e */
  function onStripWheel(e) {
    const el = scrollEl
    if (!el || e.shiftKey || Math.abs(e.deltaX) >= Math.abs(e.deltaY)) return
    if (el.scrollWidth <= el.clientWidth) return
    e.preventDefault()
    el.scrollLeft += e.deltaY
  }

  $effect(() => {
    const _id = activeTabId
    let raf = requestAnimationFrame(() => {
      const active = scrollEl?.querySelector('[aria-selected="true"]')
      active?.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' })
    })
    return () => cancelAnimationFrame(raf)
  })

  /** The tab the strip's right-click menu is for. */
  let menuTabId = $state(/** @type {string | null} */ (null))
  const menuTab = $derived(menuTabId ? tabs.find((t) => t.id === menuTabId) ?? null : null)

  /** @param {StudioTab} tab */
  function tabIcon(tab) {
    if (tab.kind === 'sql') return 'terminal'
    if (tab.kind === 'terminal') return 'square-terminal'
    if (tab.kind === 'table') {
      const entityKind = /** @type {any} */ (tab.state)?.tableKind
      // The sidebar's marks: an eye for a view, layers for a materialized view.
      if (entityKind === 'materialized_view') return 'layers'
      if (entityKind === 'view') return 'eye'
      return 'table-2'
    }
    if (tab.kind === 'ai') return 'bot'
    if (tab.kind === 'schema') return 'layout-template'
    if (tab.kind === 'orm') return 'code-2'
    if (tab.kind === 'orm-schema') return 'file-text'
    if (tab.kind === 'security') return 'shield-check'
    if (tab.kind === 'extensions') return 'blocks'
    if (tab.kind === 'map') return 'globe'
    if (tab.kind === 'search') return 'search'
    if (tab.kind === 'license') return 'key-round'
    if (tab.kind === 'shortcuts') return 'keyboard'
    if (tab.kind === 'shares') return 'share-2'
    if (tab.kind === 'queries') return 'history'
    if (tab.kind === 'migrations') return 'git-merge'
    // The rest match the command palette's icon for the same page; they all
    // fell through to a page icon, so a diagram looked like a notebook.
    const PAGE_ICONS = /** @type {Record<string, string>} */ ({
      erd: 'network',
      diagrams: 'git-branch',
      reltree: 'list-tree',
      'schema-timeline': 'git-branch',
      'data-diff': 'git-compare',
      backup: 'archive',
      json: 'braces',
      charts: 'bar-chart-2',
      dashboard: 'layout-dashboard',
      logs: 'history',
      insights: 'database',
      advisor: 'shield-check',
      objects: 'shapes',
      'extension-detail': 'blocks',
      ddl: 'file-code',
      notebook: 'notebook-pen',
      // A collection, a measurement, a label, a DynamoDB table: rows, like a table tab.
      docs: 'table-2',
      dynamodb: 'table-2',
      redis: 'key-round',
    })
    return PAGE_ICONS[tab.kind] ?? 'file-text'
  }
</script>

<header
  class="studio-chrome flex h-9 shrink-0 items-center border-b border-border/50 bg-panel px-2"
  data-studio-chrome
  data-tauri-drag-region
  role="tablist"
  tabindex="0"
  aria-label={$t('tabs.openEditors')}
  ondblclick={handleDragAreaDblClick}
>
  {#if onshowsidebar}
    <!-- With the sidebar hidden, the way back sits where it was, not only in
         the status bar's far corner. -->
    <button type="button" class={cn(navBtn, 'shrink-0')} onclick={onshowsidebar} title={withShortcut('Show sidebar', 'view.sidebar')} aria-label="Show sidebar">
      <Icon name="panel-left-open" class="size-4 shrink-0" />
    </button>
  {/if}
  {#if onback && onforward}
    <!-- Back and forward through the tabs visited (Linear's ‹ ›). Buttons as
         well as keys: Mod+[ and Mod+] stand aside inside a code editor. -->
    <div class="mr-1 flex shrink-0 items-center gap-0.5">
      <button type="button" class={navBtn} disabled={!canback} onclick={onback} title={withShortcut('Back', 'tab.back')} aria-label="Back">
        <Icon name="chevron-left" class="size-4 shrink-0" />
      </button>
      <button type="button" class={navBtn} disabled={!canforward} onclick={onforward} title={withShortcut('Forward', 'tab.forward')} aria-label="Forward">
        <Icon name="chevron-right" class="size-4 shrink-0" />
      </button>
    </div>
  {/if}
  <!-- The strip's empty stretch drags the window too (macOS, Windows, Linux):
       only an element that carries the attribute itself starts a drag, and
       this one filled the bar to the right of the last tab. -->
  <div
    bind:this={scrollEl}
    data-tauri-drag-region
    class={cn(
      // scroll-px keeps a tab scrolled into view clear of the edge fade.
      'app-scroll flex h-full min-w-0 flex-1 items-center gap-0.5 overflow-x-auto scroll-px-10 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden',
      // Clear for the first 0.75rem, then the ramp: a tab icon cut at the
      // edge still showed through a fade that started at once, and next to
      // the back and forward arrows it read as a third arrow.
      fadeLeft && fadeRight && '[mask-image:linear-gradient(to_right,transparent_0.75rem,black_2.25rem,black_calc(100%-2.25rem),transparent_calc(100%-0.75rem))]',
      fadeLeft && !fadeRight && '[mask-image:linear-gradient(to_right,transparent_0.75rem,black_2.25rem)]',
      !fadeLeft && fadeRight && '[mask-image:linear-gradient(to_right,black_calc(100%-2.25rem),transparent_calc(100%-0.75rem))]',
    )}
    onscroll={updateFades}
    onwheel={onStripWheel}
  >
    <!-- One menu for the strip, told which tab was right-clicked: a menu per
         tab mounted a bits-ui menu tree for every open tab, a dozen of them on
         a restore. The trigger is display:contents, so the strip's layout is
         the tabs'. -->
    <ContextMenu.Root>
      <ContextMenu.Trigger
        class="contents"
        oncontextmenu={(e) => {
          const id = /** @type {HTMLElement | null} */ (/** @type {Element} */ (e.target).closest('[data-tab-id]'))?.dataset.tabId
          menuTabId = id ?? null
        }}
      >
    {#each tabs as tab, i (tab.id)}
      {@const tabIconName = tabIcon(tab)}
      {@const active = tab.id === activeTabId}
        <!-- Pointer handlers for drag and middle-click; the tab itself is the
             role="tab" button inside. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          data-tab-id={tab.id}
          onpointerdown={(e) => tabPointerDown(e, tab.id)}
          onpointerup={(e) => {
            // Middle-click closes (pinned tabs stay protected, like the ✕ button)
            if (e.button === 1 && !tab.pinned) { e.preventDefault(); onclose(tab.id) }
          }}
          class={cn(
            // rem, not px: a px cap held less text at every zoom step up, and the
            // floor keeps a short title from being squeezed to "New t…".
            // A pill, not a folder tab: the active one is a soft lighter fill,
            // the rest are just text until you hover them.
            'group/tab relative flex h-6 min-w-24 max-w-50 shrink-0 items-stretch rounded-md',
            active
              ? 'bg-sidebar-accent shadow-[inset_0_0_0_1px_var(--color-border)]'
              : 'hover:bg-sidebar-accent/50',
          )}
        >
          <!-- Tab button, font-medium on ALL states to prevent layout shift on activation -->
          {#if renamingId === tab.id}
            <input
              use:focusSelect
              bind:value={renameValue}
              aria-label="Tab name"
              class="no-field-frame my-0.5 ml-1 h-5 min-w-0 flex-1 rounded-sm bg-background px-1.5 text-ui-xs text-foreground outline-none ring-1 ring-ring"
              onkeydown={(e) => {
                e.stopPropagation()
                if (e.key === 'Enter') { e.preventDefault(); commitRename(true) }
                else if (e.key === 'Escape') { e.preventDefault(); commitRename(true, false) }
              }}
              onblur={() => commitRename()}
              onpointerdown={(e) => e.stopPropagation()}
            />
          {:else}
          <button
            type="button"
            role="tab"
            aria-selected={active}
            title={tab.connLabel ? `${tabDisplayTitle(tab)} · ${tab.connLabel}` : tabDisplayTitle(tab)}
            class={cn(
              'flex min-w-0 flex-1 items-center gap-2 pl-2.5 text-left text-ui-xs leading-none transition-colors duration-100',
              // Every tab keeps the close button's slot, shown or not, so a
              // tab is the same width active, inactive and hovered: the
              // overlaid button covered the title's end on hover and the
              // tab grew when it became active.
              'pr-1',
              active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
            )}
            onclick={() => { if (_suppressTabClick) { _suppressTabClick = false; return } onselect(tab.id) }}
            ondblclick={() => startRename(tab)}
          >
            <Icon name={tabIconName} class={cn('size-3.5 shrink-0', active ? 'text-foreground' : 'text-muted-foreground')} />
            <span class="truncate leading-normal">{tabDisplayTitle(tab)}{#if tab.connLabel && clashing.has(tabDisplayTitle(tab))}<span class="text-muted-foreground">{` · ${tab.connLabel}`}</span>{/if}</span>
          </button>
          {/if}

          {#if tab.pinned}
            <!-- Pinned: pin badge replaces close, click to unpin -->
            <button
              type="button"
              class={cn(
                'hit-area mr-1 inline-flex size-5 shrink-0 self-center items-center justify-center rounded-md transition-opacity duration-100',
                'text-muted-foreground hover:bg-accent hover:text-foreground',
                active ? 'opacity-100' : 'opacity-70 group-hover/tab:opacity-100',
              )}
              title={$t('tabs.unpin')}
              aria-label={$t('tabs.unpin')}
              onclick={(e) => { e.stopPropagation(); onpintoggle(tab.id) }}
            >
              <Icon name="pin" class="size-3 shrink-0 rotate-45" />
            </button>
          {:else}
            <!-- Close button -->
            <button
              type="button"
              class={cn(
                'hit-area inline-flex size-5 shrink-0 self-center items-center justify-center rounded-md transition-opacity duration-100',
                'text-muted-foreground hover:bg-accent hover:text-foreground',
                'mr-1',
                active ? 'opacity-100' : 'opacity-0 group-hover/tab:opacity-100 focus-visible:opacity-100',
              )}
              title={$t('tabs.close')}
              aria-label={$t('tabs.close')}
              onclick={(e) => { e.stopPropagation(); onclose(tab.id) }}
            >
              <Icon name="x" class="size-3 shrink-0" />
            </button>
          {/if}

        </div>
    {/each}
    {#if onnewtab}
      <!-- Right after the last tab, where the eye already is (Linear's +). -->
      <button type="button" class={cn(navBtn, 'ml-0.5 shrink-0')} onclick={onnewtab} title={withShortcut('New tab', 'tab.new')} aria-label="New tab">
        <Icon name="plus" class="size-4 shrink-0" />
      </button>
    {/if}
      </ContextMenu.Trigger>
        <ContextMenu.Content class="min-w-52">
          {#if menuTab}
          {@const tab = menuTab}
          {@const i = tabs.findIndex((t) => t.id === tab.id)}
          {@const hasOtherClosable = tabs.some((t) => t.id !== tab.id && !t.pinned)}
          {@const leftClosable = tabs.slice(0, i).filter((t) => !t.pinned)}
          {@const rightClosable = tabs.slice(i + 1).filter((t) => !t.pinned)}
          <ContextMenu.Item onSelect={() => onpintoggle(tab.id)}>
            {#if tab.pinned}
              <Icon name="pin-off" class="size-3.5 shrink-0" />
              Unpin tab
            {:else}
              <Icon name="pin" class="size-3.5 shrink-0" />
              Pin tab
            {/if}
          </ContextMenu.Item>
          {#if tab.kind === 'table' || tab.kind === 'sql'}
            <ContextMenu.Item onSelect={() => onduplicate(tab.id)}>
              <Icon name="copy" class="size-3.5 shrink-0" />
              Duplicate tab
            </ContextMenu.Item>
          {/if}
          {#if canRename(tab)}
            <ContextMenu.Item onSelect={() => setTimeout(() => startRename(tab), 0)}>
              <Icon name="pencil" class="size-3.5 shrink-0" />
              Rename tab
            </ContextMenu.Item>
          {/if}
          {#if tab.kind === 'table'}
            <ContextMenu.Item onSelect={() => onresettable(tab.id)}>
              <Icon name="rotate-ccw" class="size-3.5 shrink-0" />
              Reset table view
            </ContextMenu.Item>
          {/if}
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => onclose(tab.id)}>
            <Icon name="x" class="size-3.5 shrink-0" />
            Close tab
            <ContextMenu.Shortcut command="tab.close" />
          </ContextMenu.Item>
          <ContextMenu.Item disabled={!hasOtherClosable} onSelect={() => oncloseothers(tab.id)}>
            <Icon name="circle-slash" class="size-3.5 shrink-0" />
            Close other tabs
          </ContextMenu.Item>
          <ContextMenu.Item
            disabled={leftClosable.length === 0}
            onSelect={() => onclosemany(leftClosable.map((t) => t.id), tab.id)}
          >
            <Icon name="arrow-left-to-line" class="size-3.5 shrink-0" />
            Close tabs to the left
          </ContextMenu.Item>
          <ContextMenu.Item
            disabled={rightClosable.length === 0}
            onSelect={() => onclosemany(rightClosable.map((t) => t.id), tab.id)}
          >
            <Icon name="arrow-right-to-line" class="size-3.5 shrink-0" />
            Close tabs to the right
          </ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item disabled={!canreopenclosed} onSelect={onreopenclosed}>
            <Icon name="history" class="size-3.5 shrink-0" />
            Reopen closed tab
            <ContextMenu.Shortcut command="tab.reopen" />
          </ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={oncloseall}>
            <Icon name="x" class="size-3.5 shrink-0" />
            Close all tabs
            <ContextMenu.Shortcut command="tab.closeAll" />
          </ContextMenu.Item>
          {/if}
        </ContextMenu.Content>
    </ContextMenu.Root>
  </div>
  <!-- Tabs scrolled out of view fade at the edge, and a tab starting inside the
       fade showed nothing at all, so the strip looked complete. While any are
       hidden, this lists every open tab. -->
  {#if fadeLeft || fadeRight}
    <!-- Searchable, and open on the tab in front: with twenty-odd tabs the
         current one was at the bottom of a plain list, out of sight. -->
    <SearchableMenu
      align="end"
      contentClass="w-80 max-w-[calc(100vw-2rem)]"
      placeholder="Search open tabs…"
      empty="No tab matches"
      items={tabs.map((tab) => ({ value: tab.id, label: tabDisplayTitle(tab), keywords: tab.connLabel ? [tab.connLabel] : [], tab }))}
      current={activeTabId ?? undefined}
      onselect={(it) => onselect(it.value)}
    >
      {#snippet trigger(props)}
        <button
          {...props}
          type="button"
          class="ml-0.5 inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent/50 hover:text-foreground"
          title="All open tabs"
          aria-label="All open tabs ({tabs.length})"
        >
          <Icon name="chevron-down" class="size-3.5 shrink-0" />
        </button>
      {/snippet}
      {#snippet item(it)}
        <Icon name={tabIcon(it.tab)} class="size-3.5 shrink-0 text-muted-foreground" />
        <span class={cn('min-w-0 flex-1 truncate', it.value === activeTabId && 'font-medium text-foreground')}>{it.label}</span>
        {#if it.tab.connLabel}<span class="ml-auto max-w-24 shrink-0 truncate text-ui-2xs text-muted-foreground">{it.tab.connLabel}</span>{/if}
        {#if it.value === activeTabId}<Icon name="check" class="size-3.5 shrink-0" />{/if}
      {/snippet}
    </SearchableMenu>
  {/if}
  <!-- A grab handle that is always there, even with the strip full of tabs. -->
  <div class="h-full w-10 shrink-0" data-tauri-drag-region aria-hidden="true"></div>
</header>
