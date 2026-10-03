<script>
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Link from '@lucide/svelte/icons/link'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ArrowRight from '@lucide/svelte/icons/arrow-right'
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right'
  import ArrowDownRight from '@lucide/svelte/icons/arrow-down-right'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import Columns3 from '@lucide/svelte/icons/columns-3'
  import Repeat from '@lucide/svelte/icons/repeat'
  import Table2 from '@lucide/svelte/icons/table-2'
  import { formatTableRowCount } from '$lib/table-list.js'

  /**
   * @typedef {{ name: string, dataType: string, isNullable: boolean,
   *   columnDefault: string|null, foreignKey: string|null,
   *   fkConstraintName: string|null, ordinalPosition: number }} Col
   * @typedef {{ name: string, columns: Col[], pkCols: Set<string>, uniqueCols?: Set<string> }} TableMeta
   */

  let {
    tableName = '',
    /** The table this row hangs off: the one whose relationships it lists. */
    parent = '',
    fromCol = '',
    toCol = '',
    direction = /** @type {'in'|'out'} */ ('out'),
    depth = 1,
    path = '',
    /** @type {Map<string, TableMeta>} */
    tableMeta = new Map(),
    /** @type {Map<string, {col:string, refTable:string, refCol:string}[]>} */
    outbound = new Map(),
    /** @type {Map<string, {fromTable:string, fromCol:string, refCol:string}[]>} */
    inbound = new Map(),
    /** @type {Set<string>} */
    expanded = new Set(),
    /** @type {Set<string>} */
    showCols = new Set(),
    /** @type {Map<string, number>} Background-filled exact row counts, keyed by table name. */
    rowCounts = new Map(),
    toggleExpand = /** @type {(k:string)=>void} */ (() => {}),
    toggleCols = /** @type {(k:string)=>void} */ (() => {}),
    onfocustable = /** @type {(name:string)=>void} */ (() => {}),
    onopentable = /** @type {((schema:string, table:string)=>void)|undefined} */ (undefined),
    activeSchema = 'public',
  } = $props()

  const meta = $derived(tableMeta.get(tableName))
  const rowCount = $derived(rowCounts.get(tableName))
  const nodeOut = $derived(outbound.get(tableName) ?? [])
  const nodeIn = $derived(inbound.get(tableName) ?? [])
  const hasMore = $derived(depth < 5 && (nodeOut.length > 0 || nodeIn.length > 0))

  const expKey = `${path}:${depth}`
  const colKey = `cols:${path}:${depth}`
  const isOpen = $derived(expanded.has(expKey))
  const colsOpen = $derived(showCols.has(colKey))
  const isOut = direction === 'out'

  // Visited tables in the current path (to detect circular refs)
  const visited = $derived(new Set(path.split(/[><:]/g).filter(Boolean)))

  /** Each level steps in by the width of the chevron, so a child's chevron
   *  sits under its parent's table icon. */
  const indent = 8 + (depth - 1) * 32
  const childIndent = 8 + depth * 32

  /** One column alone, or a single-column key, holds one row per value. */
  function isUnique(/** @type {string} */ table, /** @type {string} */ col) {
    const m = tableMeta.get(table)
    if (!m) return false
    return (m.pkCols.size === 1 && m.pkCols.has(col)) || !!m.uniqueCols?.has(col)
  }
  // The key column lives in `parent` for an outgoing link and in this table
  // for an incoming one; whether it is unique is what makes the link 1:1.
  const oneToOne = $derived(isOut ? isUnique(parent, fromCol) : isUnique(tableName, fromCol))
  const card = $derived(oneToOne ? '1:1' : isOut ? 'N:1' : '1:N')
  const cardTitle = $derived(
    oneToOne
      ? `Each ${isOut ? parent : tableName} row points at its own ${isOut ? tableName : parent} row`
      : isOut
        ? `Many ${parent} rows can point at one ${tableName} row`
        : `Many ${tableName} rows can point at one ${parent} row`,
  )
  // The parent's column stays bare - it is the table being read - and the
  // other side is named, so the pair reads the same in both sections.
  const joinFrom = $derived(isOut ? fromCol : `${tableName}.${fromCol}`)
  const joinTo = $derived(isOut ? `${tableName}.${toCol}` : toCol)
</script>

<!-- One relationship: a row in its section's group. The chevron opens the
     table's own relationships underneath, the name moves the page to it. -->
<div>
  <div class="group/rel flex h-11 items-center gap-2 pr-2 transition-colors hover:bg-muted/40" style="padding-left: {indent}px">
    {#if hasMore}
      <button
        type="button"
        aria-expanded={isOpen}
        aria-label="{isOpen ? 'Collapse' : 'Expand'} the relationships of {tableName}"
        title="{isOpen ? 'Collapse' : 'Expand'} the relationships of {tableName}"
        class="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => toggleExpand(expKey)}
      ><ChevronRight class="size-3.5 transition-transform duration-150 ease-out {isOpen ? 'rotate-90' : ''}" /></button>
    {:else}
      <span class="size-6 shrink-0"></span>
    {/if}

    <button
      type="button"
      class="flex h-full min-w-0 flex-1 items-center gap-3 text-left"
      onclick={() => onfocustable(tableName)}
      title="Explore {tableName}"
    >
      <Table2 class="size-4 shrink-0 text-muted-foreground transition-colors group-hover/rel:text-foreground" />
      <span class="min-w-0 max-w-[45%] shrink-0 truncate font-mono text-ui-sm text-foreground">{tableName}</span>
      <span class="flex min-w-0 items-center gap-1.5 font-mono text-ui-2xs text-muted-foreground" title="{isOut ? parent : tableName}.{fromCol} → {isOut ? tableName : parent}.{toCol}">
        <span class="truncate">{joinFrom}</span>
        <ArrowRight class="size-3 shrink-0" />
        <span class="truncate">{joinTo}</span>
      </span>
    </button>

    <span class="w-8 shrink-0 text-right font-mono text-ui-2xs text-muted-foreground" title={cardTitle}>{card}</span>
    <span class="w-20 shrink-0 text-right font-mono text-ui-2xs tabular-nums text-muted-foreground" title={rowCount !== undefined ? `${rowCount.toLocaleString()} row${rowCount === 1 ? '' : 's'}` : undefined}>
      {#if rowCount !== undefined}{formatTableRowCount(rowCount)} {rowCount === 1 ? 'row' : 'rows'}{/if}
    </span>
    {#if meta}
      <button
        type="button"
        aria-pressed={colsOpen}
        class="inline-flex h-6 w-12 shrink-0 items-center justify-end gap-1 rounded-md px-1.5 font-mono text-ui-2xs tabular-nums transition-colors {colsOpen ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
        onclick={() => toggleCols(colKey)}
        title="{colsOpen ? 'Hide' : 'Show'} the {meta.columns.length} columns of {tableName}"
      ><Columns3 class="size-3.5 shrink-0" />{meta.columns.length}</button>
    {/if}
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-[opacity,background-color,color] hover:bg-accent hover:text-foreground focus-visible:opacity-100 group-hover/rel:opacity-100"
      onclick={() => onopentable?.(activeSchema, tableName)}
      title="Open {tableName}"
      aria-label="Open {tableName}"
    ><ExternalLink class="size-3.5" /></button>
  </div>

  {#if colsOpen && meta}
    <div class="border-t border-border/50 py-1.5 pr-3" style="padding-left: {indent + 32}px">
      {#each meta.columns as col (col.name)}
        {@const isPk = meta.pkCols.has(col.name)}
        {@const isFk = !!col.foreignKey}
        <div class="flex h-6 items-center gap-2">
          {#if isPk}<KeyRound class="size-3.5 shrink-0 text-warning" aria-label="Primary key" />
          {:else if isFk}<Link class="size-3.5 shrink-0 text-info" aria-label="Foreign key" />
          {:else}<span class="size-3.5 shrink-0"></span>{/if}
          <span class="min-w-0 flex-1 truncate font-mono text-ui-xs {isPk ? 'text-warning' : isFk ? 'text-info' : 'text-foreground'}">{col.name}</span>
          <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{col.dataType}</span>
        </div>
      {/each}
    </div>
  {/if}

  {#if isOpen && hasMore}
    {@const shared = { tableMeta, outbound, inbound, expanded, showCols, rowCounts, toggleExpand, toggleCols, onfocustable, onopentable, activeSchema }}
    <div class="border-t border-border/50 bg-muted/[0.06]">
      {#if nodeOut.length > 0}
        <p class="flex h-7 items-end gap-1.5 pb-0.5 text-ui-2xs text-muted-foreground" style="padding-left: {childIndent + 4}px">
          <ArrowUpRight class="size-3 shrink-0 text-info" />References
        </p>
        <div class="divide-y divide-border/40">
          {#each nodeOut as rel (rel.col)}
            {@const childPath = `${path}>${rel.refTable}:${rel.col}`}
            {#if !visited.has(rel.refTable) || depth < 3}
              <svelte:self tableName={rel.refTable} parent={tableName} fromCol={rel.col} toCol={rel.refCol}
                direction="out" depth={depth + 1} path={childPath} {...shared} />
            {:else}
              <div class="flex h-9 items-center gap-2 font-mono text-ui-2xs text-muted-foreground" style="padding-left: {childIndent + 32}px">
                <Repeat class="size-3.5 shrink-0" />{rel.refTable} · already on this path
              </div>
            {/if}
          {/each}
        </div>
      {/if}
      {#if nodeIn.length > 0}
        <p class="flex h-7 items-end gap-1.5 pb-0.5 text-ui-2xs text-muted-foreground" style="padding-left: {childIndent + 4}px">
          <ArrowDownRight class="size-3 shrink-0 text-success" />Referenced by
        </p>
        <div class="divide-y divide-border/40">
          {#each nodeIn as rel (`${rel.fromTable}${rel.fromCol}`)}
            {@const childPath = `${path}<${rel.fromTable}:${rel.fromCol}`}
            {#if !visited.has(rel.fromTable) || depth < 3}
              <svelte:self tableName={rel.fromTable} parent={tableName} fromCol={rel.fromCol} toCol={rel.refCol}
                direction="in" depth={depth + 1} path={childPath} {...shared} />
            {:else}
              <div class="flex h-9 items-center gap-2 font-mono text-ui-2xs text-muted-foreground" style="padding-left: {childIndent + 32}px">
                <Repeat class="size-3.5 shrink-0" />{rel.fromTable} · already on this path
              </div>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>
