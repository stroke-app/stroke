<script>
  import { tx } from '$lib/i18n.js'
  // The window with no database open: recent connections first (one click back
  // to work), then a new connection, then engine shortcuts. Split out of
  // StudioShell; it takes what it shows and calls back for what it opens.
  import Plus from '@lucide/svelte/icons/plus'
  import { Button } from '$lib/components/ui/button/index.js'
  import Kbd from './Kbd.svelte'
  import DbIcon from './DbIcon.svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { ago } from '$lib/plugins/extensions/id-decoder.js'
  import { engineId } from '$lib/connection-provider.js'
  import { engineLabel } from '$lib/db-capabilities.js'

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection[]} */
    savedConnections = [],
    /** A dialog owns the window: the button does not take focus. */
    dialogOpen = false,
    appVersion = '',
    checkingUpdate = false,
    /** New connection, optionally for one engine. @type {(engine: string | null) => void} */
    onnew = () => {},
    /** @type {(conn: any) => void} */
    onopen = () => {},
    oncheckupdates = () => {},
    /** @type {(url: string) => void} */
    onopenlink = () => {},
    /** A database file in one step. @type {(how: 'new' | 'open') => void} */
    onfiledb = () => {},
  } = $props()

  /** The Recent list: the last four connections actually opened. */
  const recentConnections = $derived(
    savedConnections.filter((c) => c.lastConnectedAt).sort((a, b) => (b.lastConnectedAt ?? 0) - (a.lastConnectedAt ?? 0)).slice(0, 4),
  )

  /** "Redis · 127.0.0.1:6379", or the file name for local files. @param {import('$lib/stores/connections.js').SavedConnection} c */
  function welcomeTarget(c) {
    const x = /** @type {any} */ (c)
    if (x.filePath) return x.filePath === ':memory:' ? 'in memory' : String(x.filePath).split(/[\\/]/).pop() ?? ''
    if (x.url) { try { return new URL(String(x.url).replace(/^libsql:/, 'https:')).host } catch { return '' } }
    if (x.host) return `${x.host}${x.port ? `:${x.port}` : ''}`
    return engineLabel(engineId(c))
  }

  /** @type {HTMLElement | null} */
  let connectBtn = $state(null)
  // This screen has exactly one thing to do, so its button starts with focus:
  // Enter opens the dialog, Tab walks the engine chips, and nobody has to reach
  // for the mouse to get in. Skipped while a dialog owns the window.
  $effect(() => {
    if (dialogOpen || !connectBtn) return
    connectBtn.focus({ preventScroll: true })
  })
</script>

<!-- Launcher: recent connections first (one click back to work), then a
     new connection, then engine shortcuts. Left-aligned in one column so
     the eye runs straight down the list. -->
<div class="relative flex flex-1 flex-col items-center justify-center overflow-y-auto p-8">
  <div class="flex w-full max-w-[30rem] flex-col">
    <h1 class="text-ui-2xl font-semibold tracking-tight text-foreground">
      {$tx('launch.title', 'Connect a database')}
    </h1>
    <p class="mt-1 text-ui-sm text-muted-foreground text-pretty">
      {$tx('launch.sub', 'Browse schemas, edit rows and run queries in one fast, native window.')}
    </p>

    <div class="mt-6 flex items-center gap-2">
      <Button type="button" class="h-8 px-3.5 text-ui-sm font-medium" bind:ref={connectBtn} onclick={() => onnew(null)}>
        <Plus class="size-3.5 shrink-0" />
        {$tx('launch.newConnection', 'New connection')}
      </Button>
      <!-- A local file needs no form: pick where it goes (or which one) and it opens. -->
      <Button type="button" variant="outline" class="h-8 px-3 text-ui-sm" title={$tx('launch.newSqlite.title', 'Create an empty SQLite database file and open it')} onclick={() => onfiledb('new')}>
        {$tx('cmd.New SQLite database', 'New SQLite database')}
      </Button>
      <Button type="button" variant="ghost" class="h-8 px-3 text-ui-sm text-muted-foreground" title={$tx('launch.openFile.title', 'Open a SQLite or DuckDB file')} onclick={() => onfiledb('open')}>
        {$tx('launch.openFile', 'Open file…')}
      </Button>
    </div>
    <p class="mt-2 flex items-center gap-1.5 text-ui-xs text-muted-foreground">{$tx('launch.orPress', 'or press')} <Kbd command="palette.open" size="sm" /> {$tx('launch.forEverything', 'for everything else')}</p>

    <!-- Recent: one line each, name first, where it lives muted beside it.
         Two-line rows made five connections the whole page. -->
    {#if recentConnections.length}
      <section class="mt-8" aria-labelledby="welcome-recent">
        <div class="mb-1.5 flex items-center justify-between px-2.5">
          <h2 id="welcome-recent" class="text-ui-2xs font-medium text-muted-foreground">{$tx('sidebar.recent', 'Recent')}</h2>
          <button type="button" class="text-ui-2xs text-muted-foreground transition-colors hover:text-foreground" onclick={() => onnew(null)}>
            {$tx('launch.allConnections', 'All connections')}
          </button>
        </div>
        <ul class="flex flex-col">
          {#each recentConnections as conn (conn.id)}
            {@const slot = savedConnections.indexOf(conn) + 1}
            <li>
              <button
                type="button"
                title="Connect to {conn.name}"
                class="group flex h-9 w-full min-w-0 items-center gap-2.5 rounded-lg px-2.5 text-left transition-[background-color,scale] duration-150 ease-out hover:bg-accent/60 active:scale-[0.99] focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
                onclick={() => onopen(conn)}
              >
                <DbIcon id={engineId(conn)} class="size-4 shrink-0" />
                <span class="min-w-0 shrink truncate text-ui-sm text-foreground">{conn.name}</span>
                <span class="min-w-0 flex-1 truncate text-ui-xs text-muted-foreground">{welcomeTarget(conn)}</span>
                <span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground group-hover:hidden group-focus-visible:hidden">{ago(conn.lastConnectedAt ?? 0)}</span>
                <span class="hidden shrink-0 group-hover:inline-flex group-focus-visible:inline-flex">
                  {#if slot > 0 && slot <= 9}<Kbd command={`connection.switchTo.${slot}`} size="sm" />{:else}<span class="text-ui-2xs text-foreground">{$tx('connection.connect', 'Connect')}</span>{/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <!-- Each engine opens its own form directly: naming the engine is the
         first step of the wizard. -->
    <section class="mt-8" aria-labelledby="welcome-engines">
      <h2 id="welcome-engines" class="mb-1.5 px-2.5 text-ui-2xs font-medium text-muted-foreground">{$tx('launch.startWith', 'Start with')}</h2>
      <div class="grid grid-cols-2 gap-0.5 sm:grid-cols-3">
        {#each [['postgres','PostgreSQL'],['mysql','MySQL'],['sqlite','SQLite'],['mongodb','MongoDB'],['redis','Redis'],['d1','Cloudflare D1']] as [id, label] (id)}
          <button
            type="button"
            title="New {label} connection"
            class="inline-flex h-9 min-w-0 items-center gap-2.5 rounded-lg px-2.5 text-ui-sm text-muted-foreground transition-[color,background-color,scale] duration-150 ease-out hover:bg-accent/60 hover:text-foreground active:scale-[0.98] focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
            onclick={() => onnew(id)}
          >
            <DbIcon {id} class="size-4 shrink-0" />
            <span class="truncate">{label}</span>
          </button>
        {/each}
      </div>
    </section>
  </div>

  <!-- The version and a way to check for a newer one: with no
       connection open there is no sidebar, and so no update button. -->
  <p class="absolute inset-x-0 bottom-6 flex items-center justify-center gap-2 text-ui-2xs text-muted-foreground">
    {#if appVersion}
      <span class="tabular-nums">v{appVersion}</span>
      <span aria-hidden="true">·</span>
    {/if}
    <button
      type="button"
      disabled={checkingUpdate}
      class="inline-flex items-center gap-1 text-foreground/80 underline-offset-4 transition-[color,scale] duration-[120ms] ease-[var(--ease-out)] hover:text-foreground hover:underline active:scale-[0.97] disabled:pointer-events-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      onclick={oncheckupdates}
    >
      <Icon name="refresh-cw" class={cn('size-3 shrink-0', checkingUpdate && 'animate-spin')} aria-hidden="true" />
      {checkingUpdate ? $tx('launch.checking', 'Checking') : $tx('launch.checkUpdates', 'Check for updates')}
    </button>
    <span aria-hidden="true">·</span>
    <span>made with care by
    <a
      href="https://nischal-dahal.com.np"
      target="_blank"
      rel="noreferrer"
      class="text-foreground/80 underline-offset-4 transition-colors hover:text-foreground hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
      onclick={(e) => { e.preventDefault(); onopenlink('https://nischal-dahal.com.np') }}
    >@broisnees</a></span>
  </p>
</div>
