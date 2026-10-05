<script>
  import FieldSelect from './FieldSelect.svelte';
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Database from '@lucide/svelte/icons/database'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import { cn } from '$lib/utils.js'

  /**
   * @typedef {{
   *   name: string
   *   owner: string
   *   encoding: string
   *   lcCollate: string
   *   lcCtype: string
   *   template: string
   *   connectionLimit: number
   * }} CreateDbOptions
   */

  let {
    open = $bindable(false),
    /** @type {'postgres' | 'mysql' | string} */
    connType = 'postgres',
    oncreate = /** @type {(opts: CreateDbOptions) => Promise<void>} */ (async () => {}),
  } = $props()

  let name = $state('')
  /** @type {HTMLInputElement | null} */
  let nameInputEl = $state(null)
  let owner = $state('')
  let encoding = $state('UTF8')
  let lcCollate = $state('')
  let lcCtype = $state('')
  let template = $state('template0')
  let connectionLimit = $state(-1)
  let loading = $state(false)
  let error = $state('')
  let showAdvanced = $state(false)

  const isMySQL = $derived(connType === 'mysql')
  // The options below are Postgres and MySQL settings; SQL Server and ClickHouse
  // take the server's defaults, so they get the name alone.
  const hasOptions = $derived(connType === 'postgres' || connType === 'mysql')
  const engineLabel = $derived(
    ({ mysql: 'MySQL', mssql: 'SQL Server', clickhouse: 'ClickHouse' })[/** @type {string} */ (connType)] ?? 'PostgreSQL',
  )

  const PG_ENCODINGS = [
    'UTF8', 'SQL_ASCII', 'BIG5', 'EUC_CN', 'EUC_JP', 'EUC_KR', 'EUC_TW',
    'GB18030', 'GBK', 'ISO_8859_5', 'ISO_8859_6', 'ISO_8859_7', 'ISO_8859_8',
    'JOHAB', 'KOI8R', 'KOI8U', 'LATIN1', 'LATIN2', 'LATIN3', 'LATIN4',
    'LATIN5', 'LATIN6', 'LATIN7', 'LATIN8', 'LATIN9', 'LATIN10',
    'MULE_INTERNAL', 'SJIS', 'SHIFT_JIS_2004', 'UHC', 'WIN866', 'WIN874',
    'WIN1250', 'WIN1251', 'WIN1252', 'WIN1253', 'WIN1254', 'WIN1255',
    'WIN1256', 'WIN1257', 'WIN1258',
  ]

  const MYSQL_CHARSETS = [
    'utf8mb4', 'utf8', 'latin1', 'ascii', 'big5', 'binary', 'cp1250',
    'cp1251', 'cp1252', 'cp1256', 'gbk', 'gb2312', 'latin2', 'ucs2',
    'utf16', 'utf16le', 'utf32',
  ]

  const MYSQL_COLLATIONS = /** @type {Record<string, string[]>} */ ({
    utf8mb4: ['utf8mb4_unicode_ci', 'utf8mb4_general_ci', 'utf8mb4_bin', 'utf8mb4_0900_ai_ci'],
    utf8:    ['utf8_unicode_ci', 'utf8_general_ci', 'utf8_bin'],
    latin1:  ['latin1_swedish_ci', 'latin1_general_ci', 'latin1_bin'],
  })

  const mysqlCollations = $derived(MYSQL_COLLATIONS[encoding] ?? [])

  $effect(() => {
    if (!open) {
      name = ''
      owner = ''
      encoding = isMySQL ? 'utf8mb4' : 'UTF8'
      lcCollate = ''
      lcCtype = ''
      template = 'template0'
      connectionLimit = -1
      loading = false
      error = ''
      showAdvanced = false
    } else {
      // Focus the name input when the dialog opens (replaces autofocus attr)
      setTimeout(() => nameInputEl?.focus(), 0)
    }
  })

  $effect(() => {
    if (isMySQL && mysqlCollations.length && !mysqlCollations.includes(lcCollate)) {
      lcCollate = mysqlCollations[0] ?? ''
    }
  })

  /** @param {Event} e */
  async function handleSubmit(e) {
    e.preventDefault()
    const n = name.trim()
    if (!n) { error = 'Database name is required'; return }
    if (n.length > 63) { error = 'Name must be 63 characters or fewer'; return }
    if (n.includes('\0') || n.includes('"')) { error = 'Name contains invalid characters'; return }
    error = ''
    loading = true
    try {
      await oncreate({ name: n, owner: owner.trim(), encoding, lcCollate: lcCollate.trim(), lcCtype: lcCtype.trim(), template, connectionLimit })
      open = false
    } catch (err) {
      error = String(err).replace(/^Error:\s*/i, '')
    } finally {
      loading = false
    }
  }

  const inputCls = "field-surface h-8 w-full bg-background px-3 text-ui-xs outline-none placeholder:text-muted-foreground disabled:opacity-50"
  const selectCls = "field-surface h-8 w-full bg-background px-2.5 text-ui-xs outline-none disabled:opacity-50"
  const labelCls = "mb-1.5 block text-ui-xs font-medium text-muted-foreground"
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-[32.5rem] max-w-[calc(100vw-2rem)] gap-0 p-0">

    <!-- Header -->
    <div class="flex items-center gap-3 px-5 py-4 border-b border-border/60">
      <div class="flex size-8 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-muted">
        <Database class="size-3.5 text-muted-foreground" />
      </div>
      <div class="min-w-0">
        <Dialog.Title class="text-ui-sm font-semibold leading-none">Create database</Dialog.Title>
        <p class="mt-1 truncate text-ui-xs text-muted-foreground">
          {engineLabel} · on this server
        </p>
      </div>
    </div>

    <!-- Body -->
    <form onsubmit={handleSubmit} class="flex flex-col gap-0 px-5 py-4">

      <!-- Name, always visible -->
      <div class="mb-4">
        <label for="cdb-name" class="mb-1.5 block text-ui-xs font-semibold">
          Database name
          <span class="text-destructive" aria-hidden="true">*</span>
          <span class="sr-only">(required)</span>
        </label>
        <input
          id="cdb-name"
          bind:this={nameInputEl}
          type="text"
          placeholder="my_database"
          bind:value={name}
          maxlength="63"
          class={cn(inputCls, 'font-mono text-ui-sm')}
          autocomplete="off"
          spellcheck="false"
          required
          aria-describedby="cdb-name-hint"
          disabled={loading}
        />
        <p id="cdb-name-hint" class="mt-1.5 text-ui-2xs text-muted-foreground">
          Letters, digits, underscores · max 63 characters
        </p>
      </div>

      {#if hasOptions}
      <!-- Advanced toggle -->
      <button
        type="button"
        aria-expanded={showAdvanced}
        aria-controls="cdb-advanced"
        class="flex w-full items-center gap-1.5 rounded-md py-1.5 text-left text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
        onclick={() => (showAdvanced = !showAdvanced)}
      >
        <ChevronDown class={cn('size-3.5 shrink-0 transition-transform duration-150', showAdvanced && 'rotate-180')} />
        <span class="shrink-0 font-medium whitespace-nowrap">Advanced options</span>
        {#if !showAdvanced}
          <span class="ml-auto min-w-0 truncate text-ui-2xs text-muted-foreground">
            encoding, collation, owner
          </span>
        {/if}
      </button>

      <!-- Advanced fields (collapsible) -->
      {#if showAdvanced}
        <div id="cdb-advanced" class="mt-3 flex flex-col gap-3 border-t border-border/40 pt-3">

          {#if !isMySQL}
            <!-- Owner -->
            <div>
              <label for="cdb-owner" class={labelCls}>Owner</label>
              <input
                id="cdb-owner"
                type="text"
                placeholder="Leave blank for current user"
                bind:value={owner}
                class={inputCls}
                autocomplete="off"
                spellcheck="false"
                disabled={loading}
              />
            </div>
          {/if}

          <!-- Encoding + Template (PG) / Charset + Collation (MySQL) -->
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label for="cdb-enc" class={labelCls}>{isMySQL ? 'Character set' : 'Encoding'}</label>
              <FieldSelect
                id="cdb-enc"
                class="w-full text-ui-xs"
                bind:value={encoding}
                disabled={loading}
                options={(isMySQL ? MYSQL_CHARSETS : PG_ENCODINGS).map((e) => ({ value: e, label: e }))}
              />
            </div>

            {#if isMySQL}
              <div>
                <label for="cdb-collation" class={labelCls}>Collation</label>
                <FieldSelect
                  id="cdb-collation"
                  class="w-full text-ui-xs"
                  bind:value={lcCollate}
                  disabled={loading || mysqlCollations.length === 0}
                  options={mysqlCollations.length === 0
                    ? [{ value: '', label: 'Default' }]
                    : mysqlCollations.map((c) => ({ value: c, label: c }))}
                />
              </div>
            {:else}
              <div>
                <label for="cdb-template" class={labelCls}>Template</label>
                <FieldSelect
                  id="cdb-template"
                  class="w-full text-ui-xs"
                  bind:value={template}
                  disabled={loading}
                  options={[
                    { value: 'template0', label: 'template0 (clean)' },
                    { value: 'template1', label: 'template1 (default)' },
                  ]}
                />
              </div>
            {/if}
          </div>

          {#if !isMySQL}
            <!-- LC_COLLATE + LC_CTYPE -->
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label for="cdb-collate" class={labelCls}>LC_COLLATE</label>
                <input id="cdb-collate" type="text" placeholder="e.g. en_US.utf8" bind:value={lcCollate} class={cn(inputCls, 'font-mono')} autocomplete="off" spellcheck="false" disabled={loading} />
              </div>
              <div>
                <label for="cdb-ctype" class={labelCls}>LC_CTYPE</label>
                <input id="cdb-ctype" type="text" placeholder="e.g. en_US.utf8" bind:value={lcCtype} class={cn(inputCls, 'font-mono')} autocomplete="off" spellcheck="false" disabled={loading} />
              </div>
            </div>

            <!-- Connection limit -->
            <div>
              <label for="cdb-connlimit" class={labelCls}>
                Connection limit
                <span class="font-normal text-muted-foreground">(-1 = unlimited)</span>
              </label>
              <input id="cdb-connlimit" type="text" inputmode="numeric" value={connectionLimit}
                oninput={(e) => {
                  const raw = e.currentTarget.value.replace(/[^\d-]/g, '').replace(/(?!^)-/g, '')
                  e.currentTarget.value = raw
                  connectionLimit = Number(raw) || -1
                }}
                class={cn(inputCls, 'w-28 font-mono')} disabled={loading} />
            </div>
          {/if}
        </div>
      {/if}
      {/if}

      <!-- Error -->
      {#if error}
        <p class="mt-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-ui-xs text-destructive">{error}</p>
      {/if}

      <!-- Footer -->
      <div class="mt-4 flex items-center justify-end gap-2 border-t border-border/60 pt-4">
        <button
          type="button"
          class= "field-surface inline-flex h-8 items-center px-3 text-ui-xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
          onclick={() => (open = false)}
          disabled={loading}
        >
          Cancel
        </button>
        <button
          type="submit"
          class="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-4 text-ui-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
          disabled={loading || !name.trim()}
        >
          {#if loading}
            <span class="inline-block size-3 animate-spin rounded-full border border-primary-foreground border-t-transparent"></span>
            Creating…
          {:else}
            Create database
          {/if}
        </button>
      </div>
    </form>

  </Dialog.Content>
</Dialog.Root>
