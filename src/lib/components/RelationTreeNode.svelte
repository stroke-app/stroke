<script>
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Link from '@lucide/svelte/icons/link'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right'
  import ArrowDownRight from '@lucide/svelte/icons/arrow-down-right'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import Rows3 from '@lucide/svelte/icons/rows-3'
  import { formatTableRowCount } from '$lib/table-list.js'

  /**
   * @typedef {{ name: string, dataType: string, isNullable: boolean,
   *   columnDefault: string|null, foreignKey: string|null,
   *   fkConstraintName: string|null, ordinalPosition: number }} Col
   * @typedef {{ name: string, columns: Col[], pkCols: Set<string> }} TableMeta
   */

  let {
    tableName = '',
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
</script>

<!-- One relationship: a list row (DESIGN_SYSTEM §8), the table it leads to as
     the label and the column pair as its secondary line. Children nest under
     a hairline, not inside tinted boxes. -->
<div>
  <div class="group/rel flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 transition-colors hover:bg-muted/50">
    {#if isOut}
      <ArrowUpRight class="size-4 shrink-0 text-muted-foreground" aria-label="References" />
    {:else}
      <ArrowDownRight class="size-4 shrink-0 text-muted-foreground" aria-label="Referenced by" />
    {/if}

    <button
      type="button"
      class="flex min-w-0 flex-1 flex-col items-start text-left"
      onclick={() => onfocustable(tableName)}
      title="Explore {tableName}"
    >
      <span class="w-full truncate font-mono text-ui-xs text-foreground/85 group-hover/rel:text-foreground">{tableName}</span>
      <span class="w-full truncate font-mono text-ui-2xs text-muted-foreground">
        {isOut ? `${fromCol} → ${tableName}.${toCol}` : `${tableName}.${fromCol} → ${toCol}`}
      </span>
    </button>

    {#if rowCount !== undefined}
      <span class="inline-flex shrink-0 items-center gap-1 font-mono text-ui-2xs tabular-nums text-muted-foreground" title="{rowCount.toLocaleString()} row{rowCount === 1 ? '' : 's'}">
        <Rows3 class="size-3.5" />{formatTableRowCount(rowCount)}
      </span>
    {/if}
    {#if meta}
      <button
        type="button"
        aria-pressed={colsOpen}
        class="inline-flex h-7 shrink-0 items-center rounded-md px-2 font-mono text-ui-2xs tabular-nums text-muted-foreground transition-colors hover:bg-accent hover:text-foreground {colsOpen ? 'bg-accent text-foreground' : ''}"
        onclick={() => toggleCols(colKey)}
        title="{colsOpen ? 'Hide' : 'Show'} the columns of {tableName}"
      >{meta.columns.length} cols</button>
    {/if}
    {#if hasMore}
      <button
        type="button"
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={() => toggleExpand(expKey)}
        title="{isOpen ? 'Collapse' : 'Expand'} the relationships of {tableName}"
      >
        {#if isOpen}<ChevronDown class="size-3.5" />{:else}<ChevronRight class="size-3.5" />{/if}
      </button>
    {:else}
      <span class="size-7 shrink-0"></span>
    {/if}
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onclick={() => onopentable?.(activeSchema, tableName)}
      title="Open {tableName}"
    ><ExternalLink class="size-3.5" /></button>
  </div>

  {#if colsOpen && meta}
    <div class="mb-1 ml-[1.9rem] border-l border-border/50 pl-3">
      {#each meta.columns as col (col.name)}
        {@const isPk = meta.pkCols.has(col.name)}
        {@const isFk = !!col.foreignKey}
        <div class="flex h-6 items-center gap-2">
          {#if isPk}<KeyRound class="size-3.5 shrink-0 text-warning" aria-label="Primary key" />
          {:else if isFk}<Link class="size-3.5 shrink-0 text-info" aria-label="Foreign key" />
          {:else}<span class="size-3.5 shrink-0"></span>{/if}
          <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs {isPk ? 'text-warning' : isFk ? 'text-info' : 'text-foreground/85'}">{col.name}</span>
          <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{col.dataType}</span>
        </div>
      {/each}
    </div>
  {/if}

  {#if isOpen && hasMore}
    {@const shared = { tableMeta, outbound, inbound, expanded, showCols, rowCounts, toggleExpand, toggleCols, onfocustable, onopentable, activeSchema }}
    <div class="ml-[1.9rem] border-l border-border/50 pl-2">
      {#if nodeOut.length > 0}
        <p class="px-2.5 pb-0.5 pt-1.5 text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground/55">References</p>
        {#each nodeOut as rel (rel.col)}
          {@const childPath = `${path}>${rel.refTable}:${rel.col}`}
          {#if !visited.has(rel.refTable) || depth < 3}
            <svelte:self tableName={rel.refTable} fromCol={rel.col} toCol={rel.refCol}
              direction="out" depth={depth + 1} path={childPath} {...shared} />
          {:else}
            <div class="flex h-7 items-center gap-2 px-2.5 font-mono text-ui-2xs text-muted-foreground"><Link class="size-3.5" />{rel.refTable} · circular</div>
          {/if}
        {/each}
      {/if}
      {#if nodeIn.length > 0}
        <p class="px-2.5 pb-0.5 pt-1.5 text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground/55">Referenced by</p>
        {#each nodeIn as rel (`${rel.fromTable}${rel.fromCol}`)}
          {@const childPath = `${path}<${rel.fromTable}:${rel.fromCol}`}
          {#if !visited.has(rel.fromTable) || depth < 3}
            <svelte:self tableName={rel.fromTable} fromCol={rel.fromCol} toCol={rel.refCol}
              direction="in" depth={depth + 1} path={childPath} {...shared} />
          {:else}
            <div class="flex h-7 items-center gap-2 px-2.5 font-mono text-ui-2xs text-muted-foreground"><Link class="size-3.5" />{rel.fromTable} · circular</div>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>
