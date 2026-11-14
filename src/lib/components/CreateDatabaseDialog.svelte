<script>
  import { Button } from '$lib/components/ui/button/index.js'
  import FieldSelect from './FieldSelect.svelte';
  import * as Dialog from '$lib/components/ui/dialog/index.js'
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
  <!-- Same frame as What's new: title and one line under it, the body, then
       a ruled footer with one filled action. No icon tile, no rules inside. -->
  <Dialog.Content class="w-[30rem] max-w-[calc(100vw-2rem)] gap-0 p-0">
    <Dialog.Header class="gap-1 px-6 pt-6 pe-12">
      <Dialog.Title class="text-ui-lg text-balance">Create database</Dialog.Title>
      <Dialog.Description class="text-ui-xs">A new {engineLabel} database on this server.</Dialog.Description>
    </Dialog.Header>

    <form onsubmit={handleSubmit} class="flex flex-col">
      <div class="flex flex-col gap-6 px-6 pt-5 pb-6">
      <div class="flex flex-col gap-1.5">
        <label for="cdb-name" class="text-ui-xs font-medium text-foreground/90">Name</label>
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
        <p id="cdb-name-hint" class="text-ui-2xs text-muted-foreground">
          Letters, digits and underscores, up to 63 characters.
        </p>
      </div>

      {#if hasOptions}
      <!-- Advanced toggle -->
      <button
        type="button"
        aria-expanded={showAdvanced}
        aria-controls="cdb-advanced"
        class="-mb-3 flex w-fit items-center gap-1.5 rounded-md text-left text-ui-xs text-muted-foreground transition-colors duration-150 hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
        onclick={() => (showAdvanced = !showAdvanced)}
      >
        <ChevronDown class={cn('size-3.5 shrink-0 transition-transform duration-150', !showAdvanced && '-rotate-90')} />
        {isMySQL ? 'Character set and collation' : 'Encoding, collation and owner'}
      </button>

      <!-- Advanced fields (collapsible) -->
      {#if showAdvanced}
        <div id="cdb-advanced" class="flex flex-col gap-3">

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

      {#if error}
        <p role="alert" class="-mt-3 text-pretty text-ui-xs text-destructive">{error}</p>
      {/if}
      </div>

      <div class="flex items-center justify-end gap-2 border-t border-border/40 px-6 py-4">
        <Button type="button" variant="ghost" onclick={() => (open = false)} disabled={loading}>Cancel</Button>
        <Button
          type="submit"
          disabled={loading || !name.trim()}
        >
          {#if loading}
            <span class="inline-block size-3 animate-spin rounded-full border border-primary-foreground border-t-transparent"></span>
            Creating…
          {:else}
            Create database
          {/if}
        </Button>
      </div>
    </form>

  </Dialog.Content>
</Dialog.Root>
