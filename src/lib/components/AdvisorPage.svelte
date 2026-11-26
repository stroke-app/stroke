<script>
  /**
   * Advisor: what the database would tell you about itself if you asked.
   *
   * The page opens on a verdict (a 0-100 health score and the three areas it is
   * made of), then the findings GROUPED BY CHECK. A real report runs to
   * hundreds of rows; "Foreign key without an index (37)" is something you can
   * decide about, a flat wall of 235 warnings is not. Each finding can be
   * opened in the SQL editor, copied, or ignored (remembered per connection),
   * and a group's fixes come as one script.
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
    combinedFix,
    countBySeverity,
    exportRows,
    filterFindings,
    groupByCheck,
    healthScore,
    ignoreKey,
    reportSummary,
  } from '$lib/advisor.js'
  import { rowsToCsv, rowsToJson, rowsToMarkdown, saveExportFile } from '$lib/export.js'

  let {
    connectionId = '',
    /** Open SQL in the editor (a new query tab); the Advisor never runs a fix. */
    onopensql = /** @type {(sql: string) => void} */ (() => {}),
  } = $props()

  /** @type {import('$lib/api.js').AdvisorReport | null} */
  let report = $state(null)
  let loading = $state(false)
  let error = $state('')
  let scannedAt = $state(/** @type {number | null} */ (null))

  /** @type {string | null} */
  let severity = $state(null)
  /** @type {string | null} */
  let category = $state(null)
  let query = $state('')
  let showIgnored = $state(false)
  let showChecks = $state(false)
  /** Which check groups are open. */
  let expanded = $state(new Set())

  // ── Ignored findings, per connection ─────────────────────────────────────
  const ignoreStoreKey = $derived(`stroke:advisor-ignored:${connectionId || 'default'}`)
  let ignored = $state(/** @type {Set<string>} */ (new Set()))
  $effect(() => {
    const k = ignoreStoreKey
    try {
      ignored = new Set(JSON.parse(localStorage.getItem(k) ?? '[]'))
    } catch {
      ignored = new Set()
    }
  })
  /** @param {any} f */
  function toggleIgnore(f) {
    const next = new Set(ignored)
    const key = ignoreKey(f)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    ignored = next
    try { localStorage.setItem(ignoreStoreKey, JSON.stringify([...next])) } catch { /* not kept */ }
  }

  const all = $derived(report?.findings ?? [])
  const active = $derived(all.filter((f) => !ignored.has(ignoreKey(f))))
  const ignoredCount = $derived(all.length - active.length)
  const pool = $derived(showIgnored ? all : active)
  const health = $derived(healthScore(active))
  const severityCounts = $derived(countBySeverity(filterFindings(pool, { category: category ?? 'all' })))
  const visible = $derived(filterFindings(pool, { severity, category: category ?? 'all', query }))
  const groups = $derived(groupByCheck(visible))
  const checks = $derived(report?.checks ?? [])
  const failedChecks = $derived(checks.filter((c) => c.status !== 'ok'))
  const totalMs = $derived(checks.reduce((n, c) => n + (c.ms ?? 0), 0))

  /** Per area: its score and severity counts, from the findings still in play. */
  const areas = $derived(
    CATEGORIES.map((cat) => {
      const fs = active.filter((f) => f.category === cat)
      return { cat, ...healthScore(fs), counts: countBySeverity(fs), total: fs.length }
    }),
  )

  async function scan() {
    if (loading) return
    loading = true
    error = ''
    try {
      report = await advisorScan()
      scannedAt = Date.now()
      // Open the worst group so the report lands on something readable.
      const first = groupByCheck(report.findings.filter((f) => !ignored.has(ignoreKey(f))))[0]
      expanded = new Set(first ? [first.checkId] : [])
    } catch (e) {
      error = String(e)
      report = null
    } finally {
      loading = false
    }
  }

  // Scan on first open rather than on connect: a few dozen catalog queries,
  // cheap but not free, and nobody wants them on every reconnect.
  onMount(() => { void scan() })

  /** @param {string} id */
  function toggleGroup(id) {
    const next = new Set(expanded)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    expanded = next
  }

  /** @param {string} sql @param {string} [what] */
  async function copySql(sql, what = 'SQL') {
    try {
      await navigator.clipboard.writeText(sql)
      toast.success(`${what} copied`)
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
  const TONE = {
    good: { text: 'text-success', stroke: 'stroke-success', bar: 'bg-success' },
    fair: { text: 'text-warning', stroke: 'stroke-warning', bar: 'bg-warning' },
    poor: { text: 'text-destructive', stroke: 'stroke-destructive', bar: 'bg-destructive' },
  }
  /** @param {string} kind */
  const entityIcon = (kind) =>
    kind === 'index' ? 'list-tree' : kind === 'constraint' ? 'link-2' : kind === 'function' ? 'function-square' : kind === 'sequence' ? 'list-ordered' : 'table-2'
  /** @param {{ error: number, warning: number, info: number }} c */
  const countLine = (c) =>
    [c.error && `${c.error} error${c.error === 1 ? '' : 's'}`, c.warning && `${c.warning} warning${c.warning === 1 ? '' : 's'}`, c.info && `${c.info} note${c.info === 1 ? '' : 's'}`]
      .filter(Boolean).join(' · ') || 'Nothing found'
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto">
    <div class="mx-auto flex w-full max-w-5xl flex-col gap-5 px-6 py-5">
      <!-- ── Verdict ───────────────────────────────────────────────── -->
      <div class="flex flex-wrap items-center gap-5">
        <div class="relative grid size-16 shrink-0 place-items-center" aria-label="Health score {report && !report.unsupported ? health.score : 'unknown'}">
          <svg viewBox="0 0 36 36" class="absolute inset-0 size-full -rotate-90" aria-hidden="true">
            <circle cx="18" cy="18" r="15.5" fill="none" class="stroke-muted" stroke-width="3" />
            {#if report && !report.unsupported}
              <circle cx="18" cy="18" r="15.5" fill="none" stroke-width="3" stroke-linecap="round"
                class={cn('transition-[stroke-dasharray] duration-500', TONE[health.tone].stroke)}
                stroke-dasharray="{(health.score / 100) * 97.4} 97.4" />
            {/if}
          </svg>
          <span class="text-ui-lg font-semibold tabular-nums text-foreground">{report && !report.unsupported ? health.score : '–'}</span>
        </div>
        <div class="min-w-0 flex-1">
          <p class={cn('text-ui-lg font-semibold', report && !report.unsupported ? TONE[health.tone].text : 'text-foreground')}>
            {#if loading && !report}Scanning…{:else if report?.unsupported}No checks for this engine{:else if report}{health.label}{:else}Advisor{/if}
          </p>
          <p class="mt-0.5 text-ui-xs text-muted-foreground">
            {reportSummary(report)}{#if checks.length}{' · '}<span class="whitespace-nowrap">{checks.length} checks in {totalMs} ms</span>{/if}{#if scannedLabel}{' · '}<span class="whitespace-nowrap">{scannedLabel}</span>{/if}
          </p>
        </div>
        <div class="flex shrink-0 items-center gap-2">
          <Button type="button" variant="outline" size="sm" class="gap-2" disabled={loading} onclick={scan}>
            <Icon name="refresh-cw" class={cn('size-3.5 shrink-0', loading && 'animate-spin')} />
            {loading ? 'Scanning…' : 'Rescan'}
          </Button>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger>
              {#snippet child({ props })}
                <Button {...props} type="button" variant="outline" size="sm" class="gap-2" disabled={visible.length === 0}>
                  <Icon name="download" class="size-3.5 shrink-0" />
                  Export
                  <Icon name="chevron-down" class="size-3 shrink-0" />
                </Button>
              {/snippet}
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end" class="min-w-40">
              <DropdownMenu.Group>
                <DropdownMenu.GroupHeading>Export {visible.length} findings</DropdownMenu.GroupHeading>
                <DropdownMenu.Item onSelect={() => void exportReport('csv')}>CSV</DropdownMenu.Item>
                <DropdownMenu.Item onSelect={() => void exportReport('json')}>JSON</DropdownMenu.Item>
                <DropdownMenu.Item onSelect={() => void exportReport('md')}>Markdown</DropdownMenu.Item>
              </DropdownMenu.Group>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
      </div>

      {#if report && !report.unsupported}
        <!-- ── Areas: scores that are also the category filter ─────── -->
        <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
          {#each areas as a (a.cat)}
            {@const on = category === a.cat}
            <button
              type="button"
              aria-pressed={on}
              class={cn(
                'flex flex-col gap-2 rounded-lg border p-3 text-left transition-colors',
                on ? 'border-foreground/25 bg-muted/40' : 'border-border/60 hover:bg-muted/25',
              )}
              onclick={() => (category = on ? null : a.cat)}
            >
              <span class="flex items-center gap-2">
                <Icon name={CATEGORY_META[a.cat].icon} class="size-4 shrink-0 text-muted-foreground" />
                <span class="flex-1 text-ui-sm font-medium text-foreground">{CATEGORY_META[a.cat].label}</span>
                <span class={cn('text-ui-sm font-semibold tabular-nums', TONE[a.tone].text)}>{a.score}</span>
              </span>
              <span class="h-1 overflow-hidden rounded-full bg-muted" aria-hidden="true">
                <span class={cn('block h-full rounded-full transition-[width] duration-500', TONE[a.tone].bar)} style="width:{a.score}%"></span>
              </span>
              <span class="text-ui-2xs text-muted-foreground">{countLine(a.counts)}</span>
            </button>
          {/each}
        </div>

        <!-- ── Filter bar ──────────────────────────────────────────── -->
        <div class="flex flex-wrap items-center gap-1.5">
          <button
            type="button"
            aria-pressed={severity === null}
            class={cn('flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs transition-colors', severity === null ? 'bg-muted/70 font-medium text-foreground' : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground')}
            onclick={() => (severity = null)}
          >All <span class="tabular-nums opacity-60">{severityCounts.error + severityCounts.warning + severityCounts.info}</span></button>
          {#each SEVERITIES as sev (sev)}
            {@const meta = SEVERITY_META[sev]}
            <button
              type="button"
              aria-pressed={severity === sev}
              class={cn('flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs transition-colors', severity === sev ? 'bg-muted/70 font-medium text-foreground' : 'text-muted-foreground hover:bg-muted/30 hover:text-foreground')}
              onclick={() => (severity = severity === sev ? null : sev)}
            >
              <span class={cn('size-2 shrink-0 rounded-full', meta.dot)}></span>{meta.label}
              <span class="tabular-nums opacity-60">{severityCounts[sev] ?? 0}</span>
            </button>
          {/each}
          {#if category}
            <button type="button" class="flex h-7 items-center gap-1 rounded-md px-2 text-ui-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground" onclick={() => (category = null)}>
              {CATEGORY_META[category].label}<Icon name="x" class="size-3 shrink-0" />
            </button>
          {/if}
          <div class="ml-auto flex items-center gap-2">
            {#if ignoredCount > 0}
              <button type="button" class="flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground hover:bg-muted/30 hover:text-foreground" aria-pressed={showIgnored} onclick={() => (showIgnored = !showIgnored)}>
                <Icon name={showIgnored ? 'eye' : 'eye-off'} class="size-3.5 shrink-0" />{showIgnored ? 'Hide' : 'Show'} {ignoredCount} ignored
              </button>
            {/if}
            <div class="relative">
              <Icon name="search" class="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
              <input
                bind:value={query}
                placeholder="Filter by table, check…"
                aria-label="Filter findings"
                class="search-box no-field-frame h-7 w-52 pl-7 pr-2 text-ui-xs"
              />
            </div>
          </div>
        </div>
      {/if}

      <!-- ── Findings ──────────────────────────────────────────────── -->
      {#if error}
        <div class="rounded-lg border border-destructive/40 bg-destructive/5 p-3">
          <p class="text-ui-sm font-medium text-destructive">The scan failed</p>
          <p class="mt-1 font-mono text-ui-xs leading-relaxed text-muted-foreground">{error}</p>
        </div>
      {:else if loading && !report}
        <div class="loading-reveal flex flex-col items-center justify-center gap-2 py-16 text-center">
          <Icon name="loader-2" class="size-5 shrink-0 animate-spin text-muted-foreground" />
          <p class="text-ui-xs text-muted-foreground">Reading the catalog…</p>
        </div>
      {:else if report?.unsupported}
        <div class="flex flex-col items-center justify-center gap-2 py-16 text-center">
          <Icon name="info" class="size-5 shrink-0 text-muted-foreground" />
          <p class="text-ui-sm text-foreground">No checks for {report.engine} yet</p>
          <p class="max-w-md text-ui-xs leading-relaxed text-muted-foreground">
            The checks are PostgreSQL catalog queries. Rather than an empty report that reads as a clean bill of health, the Advisor says nothing until it has checks for this engine.
          </p>
        </div>
      {:else if report && groups.length === 0}
        <div class="flex flex-col items-center justify-center gap-2 rounded-lg border border-border/60 py-14 text-center">
          <Icon name="check" class="size-5 shrink-0 text-success" />
          <p class="text-ui-sm text-foreground">{active.length === 0 ? 'Nothing to report' : 'Nothing matches this filter'}</p>
          {#if active.length > 0}
            <button type="button" class="text-ui-xs text-primary hover:underline" onclick={() => { severity = null; category = null; query = '' }}>Clear filters</button>
          {/if}
        </div>
      {:else if report}
        <div class="flex flex-col gap-2">
          {#each groups as group (group.checkId)}
            {@const meta = SEVERITY_META[group.severity] ?? SEVERITY_META.info}
            {@const open = expanded.has(group.checkId)}
            {@const fix = combinedFix(group.findings)}
            <section class="overflow-hidden rounded-lg border border-border/60">
              <button
                type="button"
                class="flex w-full items-center gap-3 px-3.5 py-2.5 text-left transition-colors hover:bg-muted/20"
                aria-expanded={open}
                onclick={() => toggleGroup(group.checkId)}
              >
                <Icon name={open ? 'chevron-down' : 'chevron-right'} class="size-3.5 shrink-0 text-muted-foreground" />
                <span class={cn('size-2 shrink-0 rounded-full', meta.dot)}></span>
                <span class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground">{group.title}</span>
                <span class="hidden shrink-0 text-ui-2xs text-muted-foreground sm:inline">{CATEGORY_META[group.category]?.label ?? group.category}</span>
                <span class="shrink-0 rounded-full bg-muted/60 px-2 py-0.5 text-ui-2xs tabular-nums text-muted-foreground">{group.findings.length}</span>
              </button>

              {#if open}
                <div class="border-t border-border/40 bg-muted/10 px-3.5 py-2.5">
                  <p class="text-ui-xs leading-relaxed text-muted-foreground">{group.findings[0].description}</p>
                  {#if fix && group.findings.length > 1}
                    <div class="mt-2 flex items-center gap-1">
                      <Button type="button" variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-ui-xs" onclick={() => onopensql(fix)}>
                        <Icon name="square-terminal" class="size-3.5 shrink-0" />Open all fixes
                      </Button>
                      <Button type="button" variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-ui-xs" onclick={() => void copySql(fix, `${group.findings.length} fixes`)}>
                        <Icon name="copy" class="size-3.5 shrink-0" />Copy all
                      </Button>
                    </div>
                  {/if}
                </div>
                <ul class="divide-y divide-border/30">
                  {#each group.findings as f (ignoreKey(f))}
                    {@const isIgnored = ignored.has(ignoreKey(f))}
                    <li class={cn('group/row flex items-center gap-3 px-3.5 py-2', isIgnored && 'opacity-50')}>
                      <Icon name={entityIcon(f.entityKind)} class="size-3.5 shrink-0 text-muted-foreground" />
                      <span class="min-w-0 flex-1">
                        <span class="block truncate font-mono text-ui-xs text-foreground" title={f.entity}>{f.entity}</span>
                        {#if f.detail}<span class="mt-0.5 block truncate text-ui-2xs text-muted-foreground" title={f.detail}>{f.detail}</span>{/if}
                      </span>
                      {#if f.severity !== group.severity}
                        <span class={cn('shrink-0 text-ui-2xs', (SEVERITY_META[f.severity] ?? SEVERITY_META.info).text)}>{f.severity}</span>
                      {/if}
                      <span class="flex shrink-0 items-center gap-0.5 opacity-70 transition-opacity group-hover/row:opacity-100">
                        {#if f.remediation}
                          <button type="button" class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-2xs text-muted-foreground hover:bg-accent hover:text-foreground" title={f.remediation} onclick={() => onopensql(f.remediation ?? '')}>
                            <Icon name="square-terminal" class="size-3.5 shrink-0" />Fix in editor
                          </button>
                          <button type="button" class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Copy the fix" aria-label="Copy the fix" onclick={() => void copySql(f.remediation ?? '', 'Fix')}>
                            <Icon name="copy" class="size-3.5 shrink-0" />
                          </button>
                        {/if}
                        <button type="button" class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title={isIgnored ? 'Stop ignoring' : 'Ignore this finding'} aria-label={isIgnored ? 'Stop ignoring' : 'Ignore this finding'} onclick={() => toggleIgnore(f)}>
                          <Icon name={isIgnored ? 'eye' : 'eye-off'} class="size-3.5 shrink-0" />
                        </button>
                      </span>
                    </li>
                  {/each}
                </ul>
              {/if}
            </section>
          {/each}
        </div>
      {/if}

      <!-- ── What ran. Always shown: a report that silently drops a check it
           lacked permission for is a report that lies. ──────────────── -->
      {#if checks.length}
        <div class="rounded-lg border border-border/60">
          <button type="button" class="flex w-full items-center gap-2 px-3.5 py-2.5 text-left text-ui-xs text-muted-foreground hover:text-foreground" aria-expanded={showChecks} onclick={() => (showChecks = !showChecks)}>
            <Icon name={showChecks ? 'chevron-down' : 'chevron-right'} class="size-3.5 shrink-0" />
            {checks.length - failedChecks.length} of {checks.length} checks ran
            {#if failedChecks.length}<span class="text-warning">· {failedChecks.length} could not run</span>{/if}
          </button>
          {#if showChecks || failedChecks.length}
            <ul class="divide-y divide-border/30 border-t border-border/40">
              {#each checks as c (c.id)}
                {#if showChecks || c.status !== 'ok'}
                  <li class="flex items-center gap-2.5 px-3.5 py-1.5 text-ui-xs">
                    <Icon name={c.status === 'ok' ? 'check' : 'alert-triangle'} class={cn('size-3.5 shrink-0', c.status === 'ok' ? 'text-success' : 'text-warning')} />
                    <span class="min-w-0 flex-1 truncate text-foreground">{c.title}</span>
                    {#if c.status === 'ok'}
                      <span class="shrink-0 tabular-nums text-muted-foreground">{c.findings === 0 ? 'clean' : c.findings} · {c.ms} ms</span>
                    {:else}
                      <span class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground" title={c.error ?? ''}>{c.error ?? 'unknown error'}</span>
                    {/if}
                  </li>
                {/if}
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>
