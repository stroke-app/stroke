<script>
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { t } from '$lib/i18n.js'
  import { tabDisplayTitle } from '$lib/studio-tabs.js'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'

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
    if (/** @type {Element} */ (e.target).closest('button')) return
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().toggleMaximize()).catch(() => {})
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
    /** Open a fresh query editor - the `+` at the end of the strip. */
    onnewsql = () => {},
    /** Whether the closed-tab stack has anything to reopen. */
    canreopenclosed = false,
    onpintoggle = /** @param {string} _id */ (_id) => {},
    /** Pointer-based tab drag → split panes. Start/move/end drive the drop hints. */
    ondragtabstart = /** @param {string} _id */ (_id) => {},
    ondragtabmove = /** @type {(clientX: number, clientY: number) => void} */ (() => {}),
    ondragtabend = () => {},
  } = $props()

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
    updateFades()
    const ro = new ResizeObserver(updateFades)
    ro.observe(el)
    return () => ro.disconnect()
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

  /** @param {StudioTab} tab */
  function tabIcon(tab) {
    if (tab.kind === 'sql') return 'terminal'
    if (tab.kind === 'terminal') return 'square-terminal'
    if (tab.kind === 'table') {
      const entityKind = /** @type {any} */ (tab.state)?.tableKind
      if (entityKind === 'view' || entityKind === 'materialized_view') return 'eye'
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
    return 'file-text'
  }
</script>

<header
  class="studio-chrome flex h-10 shrink-0 items-center bg-panel px-2"
  data-studio-chrome
  data-tauri-drag-region
  role="tablist"
  tabindex="0"
  aria-label={$t('tabs.openEditors')}
  ondblclick={handleDragAreaDblClick}
>
  <div
    bind:this={scrollEl}
    class={cn(
      // scroll-px keeps a tab scrolled into view clear of the edge fade.
      'app-scroll flex h-full min-w-0 flex-1 items-center gap-0.5 overflow-x-auto scroll-px-8 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden',
      fadeLeft && fadeRight && '[mask-image:linear-gradient(to_right,transparent,black_2rem,black_calc(100%-2rem),transparent)]',
      fadeLeft && !fadeRight && '[mask-image:linear-gradient(to_right,transparent,black_2rem)]',
      !fadeLeft && fadeRight && '[mask-image:linear-gradient(to_right,black_calc(100%-2rem),transparent)]',
    )}
    onscroll={updateFades}
    onwheel={onStripWheel}
  >
    {#each tabs as tab, i (tab.id)}
      {@const tabIconName = tabIcon(tab)}
      {@const active = tab.id === activeTabId}

      <ContextMenu.Root>
        <ContextMenu.Trigger>
          {#snippet child({ props: ctxProps })}
            <div
              {...ctxProps}
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
                'group/tab relative flex h-7 min-w-24 max-w-50 shrink-0 items-stretch rounded-md transition-colors duration-100',
                active
                  ? 'bg-sidebar-accent shadow-[inset_0_0_0_1px_var(--color-border)]'
                  : 'hover:bg-sidebar-accent/50',
              )}
            >
              <!-- Tab button, font-medium on ALL states to prevent layout shift on activation -->
              <button
                type="button"
                role="tab"
                aria-selected={active}
                title={tabDisplayTitle(tab)}
                class={cn(
                  'flex min-w-0 flex-1 items-center gap-2 pl-2.5 text-left text-ui-xs leading-none transition-colors duration-100',
                  // Inactive, unpinned tabs don't reserve the close button's width
                  // (that left a dead gap after every title); the button overlays
                  // the title's end on hover, which fades out beneath it.
                  active || tab.pinned ? 'pr-1' : 'pr-3 group-hover/tab:[mask-image:linear-gradient(to_right,black_calc(100%-2.25rem),transparent_calc(100%-1rem))]',
                  active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
                )}
                onclick={() => { if (_suppressTabClick) { _suppressTabClick = false; return } onselect(tab.id) }}
              >
                <Icon name={tabIconName} class={cn('size-3.5 shrink-0', active ? 'text-foreground' : 'text-muted-foreground')} />
                <span class="truncate">{tabDisplayTitle(tab)}</span>
              </button>

              {#if tab.pinned}
                <!-- Pinned: pin badge replaces close, click to unpin -->
                <button
                  type="button"
                  class={cn(
                    'hit-area mr-1.5 inline-flex size-5 shrink-0 self-center items-center justify-center rounded-md transition-opacity duration-100',
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
                    active
                      ? 'mr-1.5 opacity-100'
                      : 'absolute right-1 top-1/2 -translate-y-1/2 opacity-0 group-hover/tab:opacity-100 focus-visible:opacity-100',
                  )}
                  title={$t('tabs.close')}
                  aria-label={$t('tabs.close')}
                  onclick={(e) => { e.stopPropagation(); onclose(tab.id) }}
                >
                  <Icon name="x" class="size-3 shrink-0" />
                </button>
              {/if}

            </div>
          {/snippet}
        </ContextMenu.Trigger>

        {@const hasOtherClosable = tabs.some((t) => t.id !== tab.id && !t.pinned)}
        {@const leftClosable = tabs.slice(0, i).filter((t) => !t.pinned)}
        {@const rightClosable = tabs.slice(i + 1).filter((t) => !t.pinned)}
        <ContextMenu.Content class="min-w-52">
          <ContextMenu.Item onSelect={() => onpintoggle(tab.id)}>
            {#if tab.pinned}
              <Icon name="pin-off" class="size-3.5 shrink-0" />
              Unpin Tab
            {:else}
              <Icon name="pin" class="size-3.5 shrink-0" />
              Pin Tab
            {/if}
          </ContextMenu.Item>
          {#if tab.kind === 'table' || tab.kind === 'sql'}
            <ContextMenu.Item onSelect={() => onduplicate(tab.id)}>
              <Icon name="copy" class="size-3.5 shrink-0" />
              Duplicate Tab
            </ContextMenu.Item>
          {/if}
          {#if tab.kind === 'table'}
            <ContextMenu.Item onSelect={() => onresettable(tab.id)}>
              <Icon name="rotate-ccw" class="size-3.5 shrink-0" />
              Reset Table View
            </ContextMenu.Item>
          {/if}
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={() => onclose(tab.id)}>
            <Icon name="x" class="size-3.5 shrink-0" />
            Close Tab
            <ContextMenu.Shortcut command="tab.close" />
          </ContextMenu.Item>
          <ContextMenu.Item disabled={!hasOtherClosable} onSelect={() => oncloseothers(tab.id)}>
            <Icon name="circle-slash" class="size-3.5 shrink-0" />
            Close Other Tabs
          </ContextMenu.Item>
          <ContextMenu.Item
            disabled={leftClosable.length === 0}
            onSelect={() => onclosemany(leftClosable.map((t) => t.id), tab.id)}
          >
            <Icon name="arrow-left-to-line" class="size-3.5 shrink-0" />
            Close Tabs to Left
          </ContextMenu.Item>
          <ContextMenu.Item
            disabled={rightClosable.length === 0}
            onSelect={() => onclosemany(rightClosable.map((t) => t.id), tab.id)}
          >
            <Icon name="arrow-right-to-line" class="size-3.5 shrink-0" />
            Close Tabs to Right
          </ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item disabled={!canreopenclosed} onSelect={onreopenclosed}>
            <Icon name="history" class="size-3.5 shrink-0" />
            Reopen last tab
            <ContextMenu.Shortcut command="tab.reopen" />
          </ContextMenu.Item>
          <ContextMenu.Separator />
          <ContextMenu.Item onSelect={oncloseall}>
            <Icon name="x" class="size-3.5 shrink-0" />
            Close All Tabs
            <ContextMenu.Shortcut command="tab.closeAll" />
          </ContextMenu.Item>
        </ContextMenu.Content>
      </ContextMenu.Root>
    {/each}
  </div>
</header>
