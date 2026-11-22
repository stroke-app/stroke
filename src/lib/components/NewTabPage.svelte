<script>
  // The new-tab page: which database this is, the search that starts
  // everything (Cmd/Ctrl+K), and a short list of where to go next. Split out of
  // StudioShell; it takes what it shows and calls back for what it opens.
  import Database from '@lucide/svelte/icons/database'
  import Terminal from '@lucide/svelte/icons/terminal'
  import SquareTerminal from '@lucide/svelte/icons/square-terminal'
  import Sparkles from '@lucide/svelte/icons/sparkles'
  import Blocks from '@lucide/svelte/icons/blocks'
  import Search from '@lucide/svelte/icons/search'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Lock from '@lucide/svelte/icons/lock'
  import Kbd from './Kbd.svelte'
  import DbIcon from './DbIcon.svelte'
  import { cn } from '$lib/utils.js'
  import { IS_MAC } from '$lib/shortcuts.js'
  import { engineId } from '$lib/connection-provider.js'
  import { engineLabel } from '$lib/db-capabilities.js'
  import { hasPro } from '$lib/stores/license.js'

  let {
    /** @type {any} */
    connection = null,
    isDoc = false,
    isDynamo = false,
    isRedis = false,
    isKV = false,
    /** @type {(page: string) => void} */
    onpalette = () => {},
    ondocs = () => {},
    onsql = () => {},
    ondynamo = () => {},
    onredis = () => {},
    onterminal = () => {},
    onai = () => {},
    onsearch = () => {},
    onextensions = () => {},
    onconnect = () => {},
  } = $props()

  const welcomeTitle = $derived(connection ? String(connection.database || connection.name || 'Database') : '')
  const welcomeWhere = $derived.by(() => {
    if (!connection) return ''
    const c = /** @type {any} */ (connection)
    let where = ''
    if (c.filePath) where = c.filePath === ':memory:' ? 'in memory' : String(c.filePath).split(/[\\/]/).pop() ?? ''
    else if (c.url) { try { where = new URL(String(c.url).replace(/^libsql:/, 'https:')).host } catch { where = '' } }
    else if (c.host) where = `${c.host}${c.port ? `:${c.port}` : ''}`
    return [engineLabel(engineId(c)), where].filter(Boolean).join(' · ')
  })

  // One platform test for the whole app: `IS_MAC` comes from `detectOs()`, which
  // reads the Tauri platform. Shift is spelled out off macOS: the bundled UI
  // and mono webfonts have no U+21E7, so "Ctrl⇧E" fell back mid-word.
  const isMac = IS_MAC
  const mod = isMac ? '⌘' : 'Ctrl'
  const shiftKey = isMac ? '⇧' : 'Shift'

  /** `Postgres · prod-db.c1uwsk8m01gi.ap-south-1.rds.amazonaws.com` → `Postgres · prod-db`: the host's first label says enough. @param {string} w */
  function shortWhere(w) {
    return String(w ?? '').replace(/([A-Za-z0-9][\w-]*)\.[\w.-]*\.(?:com|net|io|dev|cloud|app|org)\S*/, '$1')
  }
</script>

{#snippet chord(/** @type {string[]} */ keys)}
  <Kbd {keys} wrap />
{/snippet}

<!-- A tile: one line, icon and label. Compact on purpose: the two-line
     version with a description and printed chord was most of the page,
     and in the monospace font its descriptions truncated anyway. The
     description and the shortcut are in the tooltip. -->
{#snippet row(/** @type {any} */ Icon, /** @type {string} */ label, /** @type {string} */ desc, /** @type {() => void} */ onclick, /** @type {{ pro?: boolean, keys?: string[], tone?: 'info' | 'success' | 'primary' | 'warning' }} */ opts = {})}
  {@const locked = !!opts.pro && !$hasPro}
  {@const toneCls = { info: 'text-info', success: 'text-success', primary: 'text-primary', warning: 'text-warning' }[opts.tone ?? 'info']}
  {@const chordText = opts.keys ? ` (${opts.keys.join(isMac ? '' : '+')})` : ''}
  <!-- A simple action: coloured icon and label; what it does and its
       chord are in the tooltip, the chord also shows on hover. -->
  <button
    type="button"
    {onclick}
    title={locked ? `${label} - ${desc} - Pro` : `${label} - ${desc}${chordText}`}
    class={cn(
      // flex-wrap in a one-line-high, clipped box: a chord that does not
      // fit beside the label wraps out of sight instead of squeezing
      // the label ("Extensions" became "Ext..." on hover).
      "group flex h-9 min-w-0 flex-wrap content-start items-center gap-x-2.5 overflow-hidden rounded-md px-2.5 text-left text-muted-foreground outline-none transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring",
      locked && "cursor-not-allowed",
    )}
  >
    <Icon class={cn('size-4 shrink-0', toneCls)} />
    <span class="flex h-9 min-w-0 items-center truncate text-ui-sm">{label}</span>
    {#if locked}<Lock class="size-3 shrink-0 text-warning" aria-label="Pro feature" />{/if}
    <!-- In the layout only while it shows: hidden but laid out, the chord
         kept its width and cut "Search" to "Se..." at every pane size. -->
    {#if opts.keys}<span class="ml-auto hidden h-9 shrink-0 items-center group-hover:inline-flex group-focus-visible:inline-flex">{@render chord(opts.keys)}</span>{/if}
  </button>
{/snippet}

<!-- A second-tier destination: one line, no border, no chord. It has to
     read as subordinate to the tiles at a glance, which is what keeps
     the page a shortlist rather than a menu. -->
{#snippet jump(/** @type {any} */ Icon, /** @type {string} */ label, /** @type {() => void} */ onclick)}
  <button
    type="button"
    {onclick}
    class="group inline-flex h-7 min-w-0 items-center gap-1.5 rounded-md px-2 text-left text-muted-foreground outline-none hover:text-foreground focus-visible:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
  >
    <Icon class="size-4 shrink-0" />
    <span class="truncate text-ui-sm">{label}</span>
  </button>
{/snippet}

<!-- Scroll container keeps top/bottom padding reachable when the content
     outgrows the viewport (e.g. at high zoom); inner wrapper centers when it fits. -->
<div class="min-h-0 flex-1 overflow-auto">
  <!-- Everything on this page hangs off ONE leading edge, and the block
       itself is centred in the pane. It used to centre each band on its
       own axis: a centred logo over a centred label over a centred meta
       line over a left-aligned grid, so nothing lined up with anything
       and the tiles read as off-centre against the text above them.
       `max-w-xl` is the grid's width, so the header, the list and the
       footer now all start where the first tile starts. -->
  <div class="welcome-hero @container/welcome mx-auto flex min-h-full w-full max-w-2xl flex-col justify-center gap-6 px-6 pt-10 pb-[12vh]">

  {#if connection}
    <!-- Identity: which database this tab is on, as a header row that
         shares the block's left edge with everything below it. -->
    <div class="flex min-w-0 items-center gap-3">
      <span class="flex size-10 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-card">
        <DbIcon id={engineId(connection)} class="size-5 shrink-0" />
      </span>
      <div class="flex min-w-0 flex-col gap-0.5">
        <h1 class="truncate text-ui-xl font-semibold tracking-[-0.01em] text-foreground">{welcomeTitle}</h1>
        <p class="truncate text-ui-xs text-muted-foreground" title={welcomeWhere}>{shortWhere(welcomeWhere)}</p>
      </div>
    </div>
    <!-- The one main control: everything starts in ⌘K. -->
    <button
      type="button"
      class="flex h-11 w-full cursor-pointer items-center gap-3 rounded-lg bg-muted/40 px-3.5 text-left text-ui-sm text-muted-foreground transition-colors duration-150 hover:bg-muted/70 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
      onclick={() => onpalette('root')}
    >
      <Search class="size-4 shrink-0" />
      <span class="min-w-0 flex-1 truncate">Search tables, run SQL, or ask AI…</span>
      {@render chord([mod, 'K'])}
    </button>
  {/if}

  <!-- Six actions, two rows of three (one column when the pane is narrow). -->
  <div class="grid w-full grid-cols-1 gap-1 @md/welcome:grid-cols-3">
    {#if isDoc}
      {@render row(Database, "Browse", "Open collections and edit documents", ondocs, { tone: 'info' })}
      {@render row(Terminal, "Query", "Run queries in the SQL editor", onsql, { keys: [mod, shiftKey, "S"], tone: 'success' })}
    {:else if isDynamo}
      {@render row(Database, "Tables", "Scan, query and edit items", ondynamo, { tone: 'info' })}
      {@render row(Terminal, "PartiQL", "Query in the SQL editor", onsql, { keys: [mod, shiftKey, "S"], tone: 'success' })}
    {:else if isRedis}
      {@render row(KeyRound, "Keyspace", "Browse keys and values", onredis, { tone: 'info' })}
    {:else}
      {@render row(Terminal, "SQL", "Write and run a query", onsql, { keys: [mod, shiftKey, "S"], tone: 'info' })}
    {/if}
    {#if connection && !isDynamo && !isDoc}
      {@render row(SquareTerminal, "Terminal", isRedis ? "redis-cli, docked below" : "psql, mysql or sqlite3, docked below", onterminal, { keys: [IS_MAC ? "⌘" : "Ctrl", "J"], tone: 'success' })}
    {/if}
    {@render row(Sparkles, "AI", "Ask about this database", onai, { pro: true, keys: [mod, "I"], tone: 'primary' })}
    {#if connection && !isKV}
      {@render row(Search, "Search", "Find in database: search every table at once", onsearch, { keys: [mod, shiftKey, "G"], tone: 'warning' })}
    {/if}
    {@render row(Blocks, "Extensions", "Add and manage extensions", onextensions, { pro: true, keys: [mod, shiftKey, "X"], tone: 'info' })}
    {@render row(Database, "Connect", "Switch or add a connection", onconnect, { keys: [mod, shiftKey, "C"], tone: 'success' })}
  </div>

  <button type="button" class="inline-flex h-7 items-center gap-1.5 self-start rounded-md px-1 text-ui-xs text-muted-foreground transition-colors hover:text-foreground" onclick={() => onpalette('pages')}>
    Browse all pages {@render chord([mod, shiftKey, 'P'])}
  </button>
  </div>
</div>
