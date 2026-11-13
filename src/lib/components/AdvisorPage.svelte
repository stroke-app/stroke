<script>
  /**
   * Advisor: what the database would tell you about itself if you asked.
   *
   * Findings arrive flat and are shown GROUPED BY CHECK, expandable. A real report
   * runs to hundreds of rows - a flat list of "235 warnings" is a wall you scroll
   * past, while "Foreign key without an index (37)" is something you can decide
   * about. Severity tiles double as filters, so narrowing is one click from the
   * number that worried you.
   */
  import { onMount } from 'svelte'
  import Icon from './Icon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { cn } from '$lib/utils.js'
  import { advisorScan } from '$lib/api.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import {
    CATEGORIES,
    CATEGORY_META,
    SEVERITIES,
    SEVERITY_META,
    countByCategory,
    countBySeverity,
    exportRows,
    filterFindings,
    groupByCheck,
    reportSummary,
  } from '$lib/advisor.js'
  import { rowsToCsv, rowsToJson, rowsToMarkdown, saveExportFile } from '$lib/export.js'

  let { connectionId = '' } = $props()

  /** @type {import('$lib/api.js').AdvisorReport | null} */
  let report = $state(null)
  let loading = $state(false)
  let error = $state('')
  let scannedAt = $state(/** @type {number | null} */ (null))

  /** @type {string | null} */
  let severity = $state(null)
  let category = $state('all')
  let query = $state('')
  /** Which check groups are open. Collapsed by default - see the module comment. */
  let expanded = $state(new Set())

  const findings = $derived(report?.findings ?? [])
  const severityCounts = $derived(countBySeverity(findings))
  // Category counts follow the severity filter, so a tab never promises rows the
  // active filter would hide.
  const inSeverity = $derived(filterFindings(findings, { severity }))
  const categoryCounts = $derived(countByCategory(inSeverity))
  const visible = $derived(filterFindings(findings, { severity, category, query }))
  const groups = $derived(groupByCheck(visible))
  const failedChecks = $derived((report?.checks ?? []).filter((c) => c.status !== 'ok'))

  async function scan() {
    if (loading) return
    loading = true
    error = ''
    try {
      report = await advisorScan()
      scannedAt = Date.now()
      // Open the worst group so the report lands on something readable instead of
      // a list of closed rows.
      const first = groupByCheck(report.findings)[0]
      expanded = new Set(first ? [first.checkId] : [])
    } catch (e) {
      error = String(e)
      report = null
    } finally {
      loading = false
    }
  }

  // Scan on first open rather than on connect: it's a few dozen catalog queries,
  // cheap but not free, and nobody wants them on every reconnect.
  onMount(() => { void scan() })

  /** @param {string} id */
  function toggleGroup(id) {
    const next = new Set(expanded)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    expanded = next
  }

  /** @param {string} sql */
  async function copySql(sql) {
    try {
      await navigator.clipboard.writeText(sql)
      toast.success('SQL copied')
    } catch {
      toast.error('Could not copy to the clipboard')
    }
  }

  /** @param {'csv' | 'json' | 'md'} format */
  async function exportReport(format) {
    const { columns, rows } = exportRows(visible)
    if (rows.length === 0) {
      toast.info('Nothing to export with the current filter')
      return
    }
    const content =
      format === 'csv' ? rowsToCsv(columns, rows)
        : format === 'json' ? rowsToJson(columns, rows)
        : rowsToMarkdown(columns, rows)
    const stamp = new Date().toISOString().slice(0, 10)
    const saved = await saveExportFile(content, `advisor-${stamp}.${format}`, format)
    if (saved) toast.success(`Exported ${rows.length} findings`)
  }

  const scannedLabel = $derived(
    scannedAt ? new Date(scannedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '',
  )
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
  <!-- Severity tiles. Clicking one filters to it; clicking the active one clears. -->
  <div class="flex shrink-0 items-stretch gap-2 border-b border-border px-4 py-3">
    {#each SEVERITIES as sev (sev)}
      {@const meta = SEVERITY_META[sev]}
      {@const count = severityCounts[sev] ?? 0}
      <button
        type="button"
        aria-pressed={severity === sev}
        class={cn(
          'flex min-w-36 items-center gap-2.5 rounded-md border px-3 py-2 text-left transition-colors',
          severity === sev
            ? 'border-primary/40 bg-primary/5'
            : 'border-border/60 hover:bg-muted/30',
        )}
        onclick={() => (severity = severity === sev ? null : sev)}
      >
        <span class={cn('size-2.5 shrink-0 rounded-[3px]', meta.dot)}></span>
        <span class="min-w-0">
          <span class="block text-ui-sm font-medium text-foreground">{meta.label}</span>
          <span class="block text-ui-xs text-muted-foreground">
            {count === 0 ? 'none' : count}
          </span>
        </span>
      </button>
    {/each}

    <div class="ml-auto flex items-center gap-2">
      <span class="text-ui-xs text-muted-foreground">
        {reportSummary(report)}{scannedLabel ? ` · ${scannedLabel}` : ''}
      </span>
      <Button type="button" variant="outline" size="sm" class="h-8 gap-2" disabled={loading} onclick={scan}>
        <Icon name="refresh-cw" class={cn('size-3.5', loading && 'animate-spin')} />
        {loading ? 'Scanning…' : 'Refresh'}
      </Button>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button {...props} type="button" variant="outline" size="sm" class="h-8 gap-2" disabled={visible.length === 0}>
              <Icon name="download" class="size-3.5 shrink-0" />
              Export
              <Icon name="chevron-down" class="size-3 shrink-0" />
            </Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-40">
          <DropdownMenu.Group>
            <DropdownMenu.GroupHeading>
              Export {visible.length} findings
            </DropdownMenu.GroupHeading>
            <DropdownMenu.Item onSelect={() => void exportReport('csv')}>CSV</DropdownMenu.Item>
            <DropdownMenu.Item onSelect={() => void exportReport('json')}>JSON</DropdownMenu.Item>
            <DropdownMenu.Item onSelect={() => void exportReport('md')}>Markdown</DropdownMenu.Item>
          </DropdownMenu.Group>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  </div>

  <!-- Category tabs + search -->
  <div class="flex shrink-0 items-center gap-1 border-b border-border px-4 py-2">
    {#each ['all', ...CATEGORIES] as cat (cat)}
      {@const label = cat === 'all' ? 'All' : CATEGORY_META[cat].label}
      <button
        type="button"
        aria-selected={category === cat}
        class={cn(
          'flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs transition-colors',
          category === cat
            ? 'bg-muted/70 font-medium text-foreground'
            : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground',
        )}
        onclick={() => (category = cat)}
      >
        {#if cat !== 'all'}<Icon name={CATEGORY_META[cat].icon} class="size-3.5 shrink-0" />{/if}
        {label}
        <span class="tabular-nums opacity-60">{categoryCounts[cat] ?? 0}</span>
      </button>
    {/each}

    <div class="relative ml-auto">
      <Icon name="search" class="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        bind:value={query}
        placeholder="Filter by table, check…"
        aria-label="Filter findings"
        class= "field-surface h-7 w-56 bg-background pl-7 pr-2 text-ui-xs text-foreground placeholder:text-muted-foreground focus:border-primary/40 focus:outline-none"
      />
    </div>
  </div>

  <!-- Findings -->
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto">
    {#if error}
      <div class="m-4 rounded-md border border-destructive/40 bg-destructive/5 p-3">
        <p class="text-ui-sm font-medium text-destructive">The scan failed</p>
        <p class="mt-1 font-mono text-ui-xs leading-relaxed text-muted-foreground">{error}</p>
      </div>
    {:else if loading && !report}
      <div class="loading-reveal flex flex-col items-center justify-center gap-2 py-20 text-center">
        <Icon name="loader-2" class="size-5 shrink-0 animate-spin text-muted-foreground" />
        <p class="text-ui-xs text-muted-foreground">Reading the catalog…</p>
      </div>
    {:else if report?.unsupported}
      <div class="flex flex-col items-center justify-center gap-2 py-20 text-center">
        <Icon name="info" class="size-5 shrink-0 text-muted-foreground" />
        <p class="text-ui-sm text-foreground">No checks for {report.engine} yet</p>
        <p class="max-w-md text-ui-xs leading-relaxed text-muted-foreground">
          The checks are PostgreSQL catalog queries. Rather than show an empty report that
          reads as a clean bill of health, the Advisor says nothing until it has checks for
          this engine.
        </p>
      </div>
    {:else if groups.length === 0}
      <div class="flex flex-col items-center justify-center gap-2 py-20 text-center">
        <Icon name="check" class="size-5 shrink-0 text-success" />
        <p class="text-ui-sm text-foreground">
          {findings.length === 0 ? 'Nothing to report' : 'Nothing matches this filter'}
        </p>
        {#if findings.length > 0}
          <button type="button" class="text-ui-xs text-primary hover:underline" onclick={() => { severity = null; category = 'all'; query = '' }}>
            Clear filters
          </button>
        {/if}
      </div>
    {:else}
      <div class="divide-y divide-border/40">
        {#each groups as group (group.checkId)}
          {@const meta = SEVERITY_META[group.severity] ?? SEVERITY_META.info}
          {@const open = expanded.has(group.checkId)}
          <div>
            <button
              type="button"
              class="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/20"
              aria-expanded={open}
              onclick={() => toggleGroup(group.checkId)}
            >
              <Icon name={open ? 'chevron-down' : 'chevron-right'} class="size-3.5 shrink-0 text-muted-foreground" />
              <span class={cn('size-2 shrink-0 rounded-full', meta.dot)}></span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-ui-sm font-medium text-foreground">{group.title}</span>
                <span class="block text-ui-xs text-muted-foreground">
                  {CATEGORY_META[group.category]?.label ?? group.category}
                </span>
              </span>
              <span class="shrink-0 rounded-full bg-muted/60 px-2 py-0.5 font-mono text-ui-2xs tabular-nums text-muted-foreground">
                {group.findings.length}
              </span>
            </button>

            {#if open}
              <!-- The description is per check, so it goes here once rather than on
                   every row - the rows differ only in which entity they name. -->
              <p class="border-t border-border/30 bg-muted/10 px-4 py-2.5 text-ui-xs leading-relaxed text-muted-foreground">
                {group.findings[0].description}
              </p>
              <ul class="divide-y divide-border/20">
                {#each group.findings as f (f.entity)}
                  <li class="flex items-start gap-3 px-4 py-2 pl-10">
                    <Icon
                      name={f.entityKind === 'index' ? 'list' : f.entityKind === 'constraint' ? 'link-2' : 'table-2'}
                      class="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                    />
                    <span class="min-w-0 flex-1">
                      <span class="block truncate font-mono text-ui-xs text-foreground">{f.entity}</span>
                      {#if f.detail}
                        <span class="mt-0.5 block text-ui-2xs leading-relaxed text-muted-foreground">{f.detail}</span>
                      {/if}
                    </span>
                    <span class={cn('shrink-0 text-ui-2xs uppercase tracking-[0.06em]', (SEVERITY_META[f.severity] ?? SEVERITY_META.info).text)}>
                      {f.severity}
                    </span>
                    {#if f.remediation}
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        class="h-6 shrink-0 gap-1.5 px-2 text-ui-2xs"
                        title={f.remediation}
                        onclick={() => void copySql(f.remediation ?? '')}
                      >
                        <Icon name="copy" class="size-3 shrink-0" />
                        Fix SQL
                      </Button>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Checks that could not run. Shown at the bottom, always: a report that
         silently drops a check it lacked permission for is a report that lies. -->
    {#if failedChecks.length > 0}
      <div class="m-4 rounded-md border border-border/60 bg-muted/10 p-3">
        <p class="text-ui-xs font-medium text-foreground">
          {failedChecks.length} check{failedChecks.length === 1 ? '' : 's'} could not run
        </p>
        <ul class="mt-1.5 space-y-1">
          {#each failedChecks as c (c.id)}
            <li class="font-mono text-ui-2xs leading-relaxed text-muted-foreground">
              {c.title} - {c.error ?? 'unknown error'}
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>
</div>
