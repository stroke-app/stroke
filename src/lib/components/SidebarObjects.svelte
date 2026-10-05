<script module>
  /**
   * Listings by connection and schema, with the objectsVersion they were read
   * at: coming back to the tab draws at once from here, and only a bump (a
   * CREATE or DROP, Ctrl+R) or the refresh button reads the catalog again.
   * @type {Map<string, { version: number, listing: import('$lib/api.js').ObjectListing }>}
   */
  const listings = new Map()
  /** Where each connection and schema was scrolled to, for coming back. @type {Map<string, number>} */
  const scrolled = new Map()
</script>

<script>
  /**
   * The sidebar's Objects tab: one tree of views, functions, procedures,
   * triggers, sequences, types and events, whichever the engine keeps and the
   * schema holds. Kinds with nothing in them draw nothing; New creates any kind.
   *
   * The tree is one flat list of the rows on screen (objects-tree.js), drawn
   * through a window: only the rows in view and a few either side are in the
   * DOM, each at index × row height, so pg_catalog's 3,000 functions scroll
   * like twenty. Focus stays on the tree itself and `aria-activedescendant`
   * names the active row, so rows coming and going never drop it.
   */
  import { onMount, tick, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import ConfirmDialog from './ConfirmDialog.svelte'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { listDbObjects, dropDbObject } from '$lib/api.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { appSidebarRememberGroups, appNativeScroll } from '$lib/stores/settings.js'
  import { smoothScroll } from '$lib/smooth-scroll.js'
  import { FAMILY_KINDS, OBJECT_KIND_META, dropObjectSql, objectFamily, objectLabel } from '$lib/object-templates.js'
  import { TREE_KINDS, buildGroups, flattenTree, rowIndex, typeAheadIndex } from '$lib/objects-tree.js'
  import {
    objectGroups, objectsVersion, bumpObjects, openDefinition, copyDefinition, openTemplate,
  } from '$lib/stores/sidebar-objects.svelte.js'
  import { cn } from '$lib/utils.js'

  /** @typedef {import('$lib/object-templates.js').ObjectKind} ObjectKind */
  /** @typedef {import('$lib/objects-tree.js').TreeRow} TreeRow */
  /** @typedef {import('$lib/objects-tree.js').TreeItem} TreeItem */

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
    schema = '',
    /** Views and materialized views, from the sidebar's table list. @type {{ name: string, rowCount?: number | null }[]} */
    views = [],
    /** @type {{ name: string, rowCount?: number | null }[]} */
    matViews = [],
    /** The table or view open in the content area. @type {string | null} */
    activeTable = null,
    /** The sidebar's filter box (already debounced): it filters every group. */
    filter = '',
    /** Table and view comments by name (empty unless the setting is on). @type {Map<string, string>} */
    comments = new Map(),
    showComments = false,
    /** Open a view's rows. @type {(name: string) => void} */
    onopenview = () => {},
    /** Reload the sidebar's table list (it holds the views). */
    onrefreshtables = () => {},
    /** Drop a view through the table path, which closes its tabs and refreshes
     *  the list. @type {(name: string, cascade: boolean, kind: 'view' | 'materialized_view') => void} */
    ondropview = () => {},
    /** @type {(sql: import('$lib/stores/sidebar-objects.svelte.js').ObjectSql) => void} */
    onopensql = () => {},
    /** ArrowUp off the first row: back to the filter box. */
    onexittop = () => {},
    /** Rows across the tree before and after the filter, for the tab strip. */
    total = $bindable(0),
    shown = $bindable(0),
    /** True while a listing loads, for the sidebar bar's refresh spinner. */
    busyLoading = $bindable(false),
  } = $props()

  const family = $derived(objectFamily(connection?.type))
  const connKey = $derived(connection?.id ?? connection?.name ?? '')
  // The database is part of the place: switching databases on one server keeps
  // the connection id and often the schema name (public, dbo), and the cached
  // listing of the old database stayed up, its drops aimed at the new one.
  const placeKey = $derived(`${connKey}\u0000${connection?.database ?? ''}\u0000${schema}`)
  /** Kinds this engine keeps (materialized views count towards Views). */
  const engineKinds = $derived(/** @type {string[]} */ (family ? FAMILY_KINDS[family] : ['view']))
  const treeKinds = $derived(TREE_KINDS.filter((k) => engineKinds.includes(k)))
  const canCreate = $derived(!!family && !$readOnlyMode)

  $effect(() => {
    const key = connKey
    const remember = $appSidebarRememberGroups
    untrack(() => objectGroups.use(key, remember))
  })

  // ── Loading ─────────────────────────────────────────────────────────────
  /** @type {import('$lib/api.js').ObjectListing | null} */
  let listing = $state.raw(null)
  let loading = $state(false)
  let error = $state('')
  let seq = 0

  $effect(() => {
    const key = placeKey
    const version = $objectsVersion
    untrack(() => void load(key, version))
  })

  /**
   * Draw from the cache first; read the catalog when the cache is from an
   * older version (or forced). The rows on screen stay until the new ones land.
   * @param {string} key @param {number} version @param {boolean} [force]
   */
  async function load(key, version, force = false) {
    const hit = listings.get(key)
    if (hit) listing = hit.listing
    else if (!force) listing = null
    if (!force && hit && hit.version === version) return
    if (!family || !connKey) {
      listing = { kinds: [], cascade: false, objects: [] }
      return
    }
    const my = ++seq
    loading = true
    error = ''
    try {
      const res = await listDbObjects(schema)
      if (my !== seq) return
      listings.set(key, { version, listing: res })
      listing = res
    } catch (e) {
      if (my === seq) error = String(e)
    } finally {
      if (my === seq) loading = false
    }
  }

  /** Reload every group: the sidebar bar's refresh while this tab is up. */
  export function refreshAll() {
    onrefreshtables()
    void load(placeKey, $objectsVersion, true)
  }
  $effect(() => { busyLoading = loading })

  /** What the sidebar bar's + offers on this tab: the kinds this engine keeps. */
  export function newKinds() {
    if (!canCreate) return []
    return engineKinds.map((kind) => ({ kind, label: cap(one(kind)), icon: OBJECT_KIND_META[/** @type {ObjectKind} */ (kind)].icon }))
  }
  /** @param {string} kind */
  export function createKind(kind) { create(kind) }

  // ── The model ───────────────────────────────────────────────────────────
  /** Routines, triggers and the rest, strung once per listing. */
  const listingGroups = $derived(buildGroups(listing?.objects ?? []))
  /** Views come from the table list; strung when it changes. */
  const viewItems = $derived(
    buildGroups([
      ...views.map((v) => ({ kind: 'view', name: v.name, comment: comments.get(v.name) ?? null })),
      ...matViews.map((v) => ({ kind: 'matview', name: v.name, comment: comments.get(v.name) ?? null, rowCount: v.rowCount })),
    ]).get('view') ?? [],
  )
  const groups = $derived(new Map([...listingGroups, ['view', viewItems]]))
  const q = $derived(filter.trim().toLowerCase())
  const flat = $derived(flattenTree(groups, treeKinds, q, (k) => objectGroups.isOpen(k)))
  const rows = $derived(flat.rows)
  $effect(() => {
    total = flat.total
    shown = flat.shown
  })
  /** Whether everything that can be listed has been (views are always in). */
  const settled = $derived(!!listing || !family)

  // ── Window ──────────────────────────────────────────────────────────────
  let scroller = $state(/** @type {HTMLDivElement | null} */ (null))
  let probe = $state(/** @type {HTMLDivElement | null} */ (null))
  /** One row, in px, measured: h-7 follows the zoom setting, so it is not a constant. */
  let rowH = $state(28)
  let scrollTop = $state(0)
  let viewH = $state(400)
  const OVERSCAN = 8
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN))
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewH) / rowH) + OVERSCAN))
  const visible = $derived(rows.slice(start, end))

  let scrollFrame = 0
  function onScroll() {
    if (scrollFrame) return
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = 0
      if (!scroller) return
      scrollTop = scroller.scrollTop
      scrolled.set(placeKey, scrollTop)
    })
  }

  onMount(() => {
    const measure = () => {
      const h = probe?.getBoundingClientRect().height
      if (h && h > 0) rowH = h
      if (scroller) viewH = scroller.clientHeight
    }
    measure()
    const ro = new ResizeObserver(measure)
    if (probe) ro.observe(probe)
    if (scroller) ro.observe(scroller)
    // Back where this schema was left.
    const at = scrolled.get(placeKey)
    if (at && scroller) {
      scroller.scrollTop = at
      scrollTop = scroller.scrollTop
    }
    return () => {
      ro.disconnect()
      if (scrollFrame) cancelAnimationFrame(scrollFrame)
    }
  })

  // ── Focus, selection, keyboard ──────────────────────────────────────────
  /** The active row: the one clicked or arrowed to. Kept by key, so it survives a refresh. */
  let active = $state('')
  /** While the keyboard drives: the active row wears the focus ring. A pointer clears it. */
  let kbd = $state(false)
  let focused = $state(false)
  const activeIndex = $derived(rowIndex(rows, active))
  /** @param {number} i */
  const rowId = (i) => `objects-row-${i}`

  /** @param {number} i */
  function ensureVisible(i) {
    if (!scroller) return
    const top = i * rowH
    if (top < scroller.scrollTop) scroller.scrollTop = top
    else if (top + rowH > scroller.scrollTop + scroller.clientHeight) scroller.scrollTop = top + rowH - scroller.clientHeight
    scrollTop = scroller.scrollTop
  }

  /** @param {number} i */
  function setActive(i) {
    const r = rows[Math.max(0, Math.min(rows.length - 1, i))]
    if (!r) return
    active = r.key
    ensureVisible(rowIndex(rows, r.key))
  }

  /** @param {TreeRow} r */
  function toggle(r) {
    if (r.t === 'item' || q) return
    objectGroups.toggle(r.key, !r.open)
  }

  /** @param {TreeItem} item */
  async function openItem(item) {
    const o = item.obj
    if (o.kind === 'view' || o.kind === 'matview') onopenview(o.name)
    else await showDefinition(item)
  }

  /** The row whose definition is loading. */
  let busy = $state('')

  /** @param {TreeItem} item */
  async function showDefinition(item) {
    busy = item.key
    try {
      await openDefinition(schema, item.obj, onopensql)
    } finally {
      if (busy === item.key) busy = ''
    }
  }

  /** @param {string} kind */
  function create(kind) {
    if ($readOnlyMode) {
      guardWrite(`create a ${OBJECT_KIND_META[/** @type {ObjectKind} */ (kind)]?.one ?? kind}`)
      return
    }
    openTemplate(connection?.type, /** @type {ObjectKind} */ (kind), schema, onopensql)
  }

  let typed = ''
  let typedAt = 0

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (!rows.length) return
    const i = activeIndex < 0 ? 0 : activeIndex
    const r = rows[i]
    const page = Math.max(1, Math.floor(viewH / rowH) - 1)
    const stop = () => { e.preventDefault(); e.stopPropagation(); kbd = true }
    if ((e.key === 'F10' && e.shiftKey) || e.key === 'ContextMenu') {
      stop()
      setActive(i)
      void tick().then(() => {
        const el = scroller?.querySelector(`#${rowId(i)}`)
        if (!el) return
        const box = el.getBoundingClientRect()
        el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: box.left + 32, clientY: box.bottom - 4 }))
      })
      return
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return
    switch (e.key) {
      case 'ArrowDown': stop(); setActive(activeIndex < 0 ? 0 : i + 1); return
      case 'ArrowUp':
        stop()
        if (i === 0) { kbd = false; onexittop() } else setActive(i - 1)
        return
      case 'Home': stop(); setActive(0); return
      case 'End': stop(); setActive(rows.length - 1); return
      case 'PageDown': stop(); setActive(i + page); return
      case 'PageUp': stop(); setActive(i - page); return
      case 'ArrowRight':
        stop()
        if (r.t === 'item') return
        if (!r.open) toggle(r)
        else if (rows[i + 1] && rows[i + 1].depth > r.depth) setActive(i + 1)
        return
      case 'ArrowLeft': {
        stop()
        if (r.t !== 'item' && r.open && !q) { toggle(r); return }
        const parent = r.t === 'item' ? r.parent : r.t === 'ext' ? `g:${r.group}` : ''
        const p = rowIndex(rows, parent)
        if (p >= 0) setActive(p)
        return
      }
      case 'Enter':
        stop()
        if (r.t === 'item') void openItem(r.item)
        else toggle(r)
        return
      case ' ':
        stop()
        if (r.t !== 'item') toggle(r)
        return
    }
    // Type-ahead: letters jump to the next row whose name starts with them.
    if (e.key.length === 1) {
      stop()
      const now = performance.now()
      typed = now - typedAt > 700 ? e.key : typed + e.key
      typedAt = now
      const hit = typeAheadIndex(rows, i, typed)
      if (hit >= 0) setActive(hit)
    }
  }

  /** @param {MouseEvent} e */
  function rowAt(e) {
    const el = e.target instanceof Element ? /** @type {HTMLElement | null} */ (e.target.closest('[data-index]')) : null
    const i = el ? Number(el.dataset.index) : -1
    return i >= 0 ? { i, r: rows[i] } : null
  }

  /** @param {MouseEvent} e */
  function onClick(e) {
    const hit = rowAt(e)
    if (!hit) return
    kbd = false
    active = hit.r.key
    scroller?.focus({ preventScroll: true })
    if (hit.r.t !== 'item') toggle(hit.r)
  }

  /** @param {MouseEvent} e */
  function onDblClick(e) {
    const hit = rowAt(e)
    if (hit?.r.t === 'item') void openItem(hit.r.item)
  }

  /** Focus the tree from outside (ArrowDown in the filter box). */
  export function focusTree() {
    if (!scroller || !rows.length) return false
    if (activeIndex < 0) active = rows.find((r) => r.t === 'item')?.key ?? rows[0].key
    kbd = true
    scroller.focus({ preventScroll: true })
    ensureVisible(Math.max(0, rowIndex(rows, active)))
    return true
  }

  /** Enter in the filter box: open the one match, if the filter left exactly one. */
  export function openSole() {
    if (!q || flat.shown !== 1) return false
    const r = rows.find((x) => x.t === 'item')
    if (r?.t !== 'item') return false
    void openItem(r.item)
    return true
  }

  // ── Context menu (one, for the row under the pointer) ───────────────────
  /** @type {TreeRow | null} */
  let menuRow = $state(null)

  // ── Drop ────────────────────────────────────────────────────────────────
  let dropOpen = $state(false)
  let dropCascade = $state(false)
  /** @type {TreeItem | null} */
  let dropTarget = $state(null)
  const canCascade = $derived(!!listing?.cascade)
  const dropSql = $derived(dropTarget ? dropObjectSql(connection?.type, schema, dropTarget.obj, canCascade && dropCascade) : null)

  /** @param {TreeItem} item */
  function askDrop(item) {
    if (!guardWrite(`drop this ${OBJECT_KIND_META[/** @type {ObjectKind} */ (item.kind)]?.one ?? item.kind}`)) return
    dropTarget = item
    dropCascade = false
    dropOpen = true
  }

  async function confirmDrop() {
    const item = dropTarget
    if (!item) return
    try {
      if (item.kind === 'view' || item.kind === 'matview') {
        // The table path: it says so, closes the view's open tabs and re-lists.
        ondropview(item.name, canCascade && dropCascade, item.kind === 'matview' ? 'materialized_view' : 'view')
      } else {
        const ran = await dropDbObject(schema, item.obj, canCascade && dropCascade)
        toast.success(`Dropped ${objectLabel(item.obj)}`, { description: ran })
      }
      bumpObjects()
    } catch (e) {
      toast.error(`Couldn't drop ${item.name}`, { description: String(e) })
    }
  }

  /** @param {string} text */
  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text)
      toast.success('Copied')
    } catch (e) {
      toast.error('Copy failed', { description: String(e) })
    }
  }

  /** The filter's match inside a name. @param {string} name */
  function parts(name) {
    if (!q) return null
    const i = name.toLowerCase().indexOf(q)
    if (i < 0) return null
    return [name.slice(0, i), name.slice(i, i + q.length), name.slice(i + q.length)]
  }

  /** @param {ObjectKind | string} kind */
  const one = (kind) => OBJECT_KIND_META[/** @type {ObjectKind} */ (kind)]?.one ?? kind
  /** @param {string} s */
  const cap = (s) => s[0].toUpperCase() + s.slice(1)

  // The Tables tab's row recipe (Sidebar.svelte), so both tabs read as one.
  const ROW = 'absolute inset-x-1.5 flex h-7 cursor-default select-none items-center rounded-md text-left transition-colors'
  const IDLE = 'text-foreground/70 hover:bg-sidebar-accent/50 hover:text-foreground'
  const SELECTED = 'bg-sidebar-accent text-sidebar-accent-foreground'
  const ICON_BTN = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
</script>

{#snippet newMenu(/** @type {'header' | 'empty'} */ where)}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger
      class={where === 'header'
        ? cn(ICON_BTN, 'w-auto gap-0.5 px-1.5 data-[state=open]:bg-accent data-[state=open]:text-foreground')
        : 'field-surface inline-flex h-7 items-center gap-1.5 px-2.5 text-ui-xs text-foreground transition-colors hover:bg-accent data-[state=open]:bg-accent'}
      title={$readOnlyMode ? READ_ONLY_HINT : 'New view, function, trigger…'}
      aria-label="New object"
      disabled={!family || $readOnlyMode}
    >
      <Icon name="plus" class="size-3.5 shrink-0" />
      {#if where === 'empty'}New{/if}
      <Icon name="chevron-down" class="size-3 shrink-0" />
    </DropdownMenu.Trigger>
    <DropdownMenu.Content align="end" class="min-w-44">
      {#each engineKinds as kind (kind)}
        <DropdownMenu.Item onSelect={() => create(kind)}>
          <Icon name={OBJECT_KIND_META[/** @type {ObjectKind} */ (kind)].icon} />
          {cap(one(kind))}
        </DropdownMenu.Item>
      {/each}
    </DropdownMenu.Content>
  </DropdownMenu.Root>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  <!-- New and refresh live in the sidebar's own bar on this tab, as they do
       on Tables: no second header row inside the list. -->

  {#if settled && !error && flat.total === 0}
    <!-- Nothing at all in the schema: one place to start from. -->
    <div class="flex flex-col items-center gap-3 px-6 py-14 text-center">
      <div class="flex size-10 items-center justify-center rounded-lg border border-border bg-muted/30">
        <Icon name="blocks" class="size-5 text-muted-foreground" />
      </div>
      <p class="max-w-[16rem] text-ui-xs font-medium text-foreground">No views or routines in “{schema}”</p>
      {#if family}{@render newMenu('empty')}{/if}
    </div>
  {:else}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        {#snippet child({ props })}
          {@const openMenu = props.oncontextmenu}
          <div
            {...props}
            bind:this={scroller}
            role="tree"
            tabindex="0"
            aria-label="Objects in {schema}"
            aria-activedescendant={activeIndex >= start && activeIndex < end ? rowId(activeIndex) : undefined}
            class="app-scroll relative min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain outline-none"
            use:smoothScroll={{ enabled: !$appNativeScroll }}
            onscroll={onScroll}
            onkeydown={onKey}
            onfocus={() => (focused = true)}
            onblur={() => (focused = false)}
            onpointerdown={(e) => { kbd = false; props.onpointerdown?.(e) }}
            onclick={onClick}
            ondblclick={onDblClick}
            oncontextmenu={(e) => {
              const hit = rowAt(e)
              if (!hit) { e.preventDefault(); return }
              menuRow = hit.r
              active = hit.r.key
              openMenu?.(e)
            }}
          >
            <div bind:this={probe} class="pointer-events-none invisible absolute h-7 w-px" aria-hidden="true"></div>
            <div class="relative w-full" style="height: {rows.length * rowH}px">
              {#each visible as r, n (r.key)}
                {@const i = start + n}
                {@const isActive = r.key === active}
                {@const ring = isActive && focused && kbd}
                {#if r.t === 'group'}
                  <div
                    id={rowId(i)}
                    role="treeitem"
                    aria-level={1}
                    aria-expanded={r.open}
                    aria-selected={isActive}
                    data-index={i}
                    class={cn(ROW, 'group/row gap-1.5 px-2', isActive ? SELECTED : 'hover:bg-sidebar-accent/50', ring && 'outline-2 -outline-offset-2 outline-ring')}
                    style="top: {i * rowH}px"
                  >
                    <!-- A tree node: chevron, the kind's coloured icon, its name and
                         the count in brackets. -->
                    <Icon name="chevron-right" class={cn('size-3 shrink-0 text-muted-foreground transition-transform duration-150', r.open && 'rotate-90')} />
                    <Icon name={OBJECT_KIND_META[/** @type {ObjectKind} */ (r.group)]?.icon ?? 'box'} class={cn('size-4 shrink-0', OBJECT_KIND_META[/** @type {ObjectKind} */ (r.group)]?.tone)} />
                    <span class="min-w-0 truncate text-ui-sm text-foreground">{r.label}</span>
                    <span class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground">
                      ({q ? `${r.count}/${r.total}` : r.total})
                    </span>
                    {#if canCreate}
                      <!-- Over the count, which hides while it shows: nothing moves. -->
                      <button
                        type="button"
                        tabindex="-1"
                        class={cn('absolute right-1 top-1/2 inline-flex size-5 -translate-y-1/2 items-center justify-center rounded-md text-muted-foreground opacity-0 hover:bg-accent hover:text-foreground group-hover/row:opacity-100', ring && 'opacity-100')}
                        title="New {one(r.group)}"
                        aria-label="New {one(r.group)}"
                        onclick={(e) => { e.stopPropagation(); create(r.group) }}
                      >
                        <Icon name="plus" class="size-3.5" />
                      </button>
                    {/if}
                  </div>
                {:else if r.t === 'ext'}
                  <div
                    id={rowId(i)}
                    role="treeitem"
                    aria-level={2}
                    aria-expanded={r.open}
                    aria-selected={isActive}
                    data-index={i}
                    class={cn(ROW, 'gap-1.5 pl-9 pr-2', isActive ? SELECTED : IDLE, ring && 'outline-2 -outline-offset-2 outline-ring')}
                    style="top: {i * rowH}px"
                    title="Objects the {r.ext} extension owns"
                  >
                    <Icon name="chevron-right" class={cn('size-3 shrink-0 text-muted-foreground transition-transform duration-150', r.open && 'rotate-90')} />
                    <Icon name="package" class="size-3.5 shrink-0 text-muted-foreground" />
                    <span class="min-w-0 truncate text-ui-sm">{r.label}</span>
                    <span class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground">({q ? `${r.count}/${r.total}` : r.total})</span>
                  </div>
                {:else}
                  {@const item = r.item}
                  {@const hit = parts(item.name)}
                  {@const current = (item.kind === 'view' || item.kind === 'matview') && activeTable === item.name}
                  <div
                    id={rowId(i)}
                    role="treeitem"
                    aria-level={r.depth + 1}
                    aria-selected={isActive}
                    data-index={i}
                    class={cn(ROW, 'gap-1.5 pr-2', r.depth === 2 ? 'pl-[3.375rem]' : 'pl-9', isActive || current ? SELECTED : IDLE, ring && 'outline-2 -outline-offset-2 outline-ring')}
                    style="top: {i * rowH}px"
                    title={item.title}
                  >
                    <Icon name={OBJECT_KIND_META[/** @type {ObjectKind} */ (item.kind)]?.icon ?? 'box'} class={cn('size-3.5 shrink-0', OBJECT_KIND_META[/** @type {ObjectKind} */ (item.kind)]?.tone)} />
                    <span class="flex min-w-0 flex-1 items-baseline font-mono text-ui-sm leading-4">
                      <span class="max-w-full shrink-0 truncate">{#if hit}{hit[0]}<mark class="rounded-[2px] bg-primary/25 text-inherit">{hit[1]}</mark>{hit[2]}{:else}{item.name}{/if}</span>
                      {#if item.kind === 'function' || item.kind === 'procedure'}
                        <span class="min-w-0 truncate pl-1.5 text-ui-xs text-muted-foreground">({item.args})</span>
                      {/if}
                      {#if showComments && item.comment}
                        <span class="min-w-0 truncate pl-2 font-sans text-ui-2xs text-muted-foreground">{item.comment}</span>
                      {/if}
                    </span>
                    {#if busy === item.key}
                      <Icon name="loader-2" class="size-3 shrink-0 animate-spin text-muted-foreground" />
                    {:else if item.right}
                      <span class="max-w-[45%] shrink-0 truncate text-right font-mono text-ui-xs leading-4 tabular-nums text-muted-foreground">{item.right}</span>
                    {/if}
                  </div>
                {/if}
              {/each}
            </div>
            {#if q && rows.length === 0}
              <p class="px-3.5 py-1.5 text-ui-2xs text-muted-foreground">Nothing matches “{filter.trim()}”</p>
            {/if}
            {#if error}
              <p class="px-3.5 py-1.5 text-ui-2xs text-destructive">
                {error}
                <button type="button" class="ml-1 underline hover:text-foreground" onclick={refreshAll}>Retry</button>
              </p>
            {:else if !settled}
              <p class="flex items-center gap-1.5 px-3.5 py-1.5 text-ui-2xs text-muted-foreground">
                <Icon name="loader-2" class="size-3 shrink-0 animate-spin" />Reading the schema
              </p>
            {/if}
          </div>
        {/snippet}
      </ContextMenu.Trigger>
      <ContextMenu.Content class="min-w-48">
        {#if menuRow?.t === 'item'}
          {@const item = menuRow.item}
          {@const owned = item.ext}
          {#if item.kind === 'view' || item.kind === 'matview'}
            <ContextMenu.Item onSelect={() => onopenview(item.name)}>
              <Icon name="table-2" />
              Open rows
            </ContextMenu.Item>
          {/if}
          {#if family}
            <ContextMenu.Item onSelect={() => void showDefinition(item)}>
              <Icon name="file-code" />
              Open definition
            </ContextMenu.Item>
          {/if}
          <ContextMenu.Item onSelect={() => void copyText(item.name)}>
            <Icon name="copy" />
            Copy name
          </ContextMenu.Item>
          {#if family}
            <ContextMenu.Item onSelect={() => void copyDefinition(schema, item.obj)}>
              <Icon name="copy" />
              Copy definition
            </ContextMenu.Item>
            <ContextMenu.Separator />
            <ContextMenu.Item
              variant="destructive"
              disabled={$readOnlyMode || !!owned}
              title={owned ? `Owned by the ${owned} extension; DROP EXTENSION removes it` : $readOnlyMode ? READ_ONLY_HINT : undefined}
              onSelect={() => askDrop(item)}
            >
              <Icon name="trash-2" />
              Drop {one(item.kind)}
            </ContextMenu.Item>
            {#if owned}
              <p class="px-2 pb-1 text-ui-2xs text-muted-foreground">Owned by the {owned} extension</p>
            {/if}
          {/if}
        {:else if menuRow}
          {@const group = menuRow.group}
          {#if canCreate}
            <ContextMenu.Item onSelect={() => create(group)}>
              <Icon name="plus" />
              New {one(group)}
            </ContextMenu.Item>
          {/if}
          <ContextMenu.Item onSelect={refreshAll}>
            <Icon name="refresh-cw" />
            Refresh
          </ContextMenu.Item>
        {/if}
      </ContextMenu.Content>
    </ContextMenu.Root>
  {/if}
</div>

{#snippet dropExtra()}
  {#if canCascade}
    <div class="flex items-center justify-between gap-4 px-5 py-3.5">
      <div>
        <p class="text-ui-xs font-medium text-foreground">Cascade</p>
        <p class="mt-0.5 text-ui-2xs text-muted-foreground">Also drop everything that depends on it</p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={dropCascade}
        aria-label="Cascade"
        class={cn(
          'relative inline-flex h-[18px] w-8 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
          dropCascade ? 'bg-destructive' : 'bg-muted',
        )}
        onclick={() => (dropCascade = !dropCascade)}
      >
        <span class={cn(
          'pointer-events-none block size-3.5 rounded-full bg-white shadow-sm transition-transform duration-200',
          dropCascade ? 'translate-x-[14px]' : 'translate-x-0',
        )}></span>
      </button>
    </div>
    <div class="h-px bg-border/25"></div>
  {/if}
  {#if dropSql}
    <div class="px-5 py-4">
      <p class="mb-2 text-ui-3xs font-semibold uppercase tracking-[0.07em] text-muted-foreground">Will execute</p>
      <div class="rounded-lg border border-border/20 bg-muted/[0.3] px-3.5 py-2.5">
        <code class="break-all font-mono text-ui-xs text-destructive">{dropSql}</code>
      </div>
    </div>
  {/if}
{/snippet}

<ConfirmDialog
  bind:open={dropOpen}
  icon="alert-triangle"
  title="Drop {dropTarget ? one(dropTarget.kind) : ''}"
  description={dropTarget ? `${objectLabel(dropTarget.obj)} is removed for good. Anything that uses it fails until it is created again.` : ''}
  confirmLabel="Drop"
  confirmIcon="trash-2"
  variant="destructive"
  extra={dropExtra}
  onconfirm={() => void confirmDrop()}
/>
