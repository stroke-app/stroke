<script>
  // First-run tour, T3-style: one quiet column, two steps, nothing to read
  // twice. Licensing is not a step any more - sign-in happens before the app
  // opens (SignInScreen), so the tour only says what Stroke is and gets you to
  // a database.
  import { fade } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import Kbd from './Kbd.svelte'
  import DbIcon from './DbIcon.svelte'
  import { engineTint } from '$lib/engine-tint.js'
  import { cn } from '$lib/utils.js'
  import AccountSignInButtons from './AccountSignInButtons.svelte'
  import AccountAvatar from './AccountAvatar.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { account } from '$lib/stores/account.js'

  let { open = $bindable(false), onconnect = () => {}, onsample = () => {} } = $props()

  const KEY = 'stroke:onboarded'
  const TOTAL = 3
  let step = $state(1)

  const isMac = typeof document !== 'undefined' && document.documentElement.dataset.os === 'macos'

  // Everything the connection dialog can open.
  const ENGINES = [
    ['postgres', 'Postgres'], ['mysql', 'MySQL'], ['mariadb', 'MariaDB'], ['sqlite', 'SQLite'], ['mssql', 'SQL Server'], ['oracle', 'Oracle'],
    ['cockroachdb', 'CockroachDB'], ['duckdb', 'DuckDB'], ['clickhouse', 'ClickHouse'],
    ['mongodb', 'MongoDB'], ['redis', 'Redis'], ['valkey', 'Valkey'], ['dynamodb', 'DynamoDB'], ['couchdb', 'CouchDB'],
    ['cassandra', 'Cassandra'], ['firestore', 'Firestore'],
    ['d1', 'Cloudflare D1'], ['supabase', 'Supabase'], ['neon', 'Neon'], ['planetscale', 'PlanetScale'], ['turso', 'Turso'],
    ['prisma', 'Prisma Postgres'], ['tidb', 'TiDB Cloud'], ['upstash', 'Upstash'], ['railway', 'Railway'],
  ]

  const FEATURES = [
    { icon: 'table-2', title: 'Browse and edit rows', desc: 'Filter, sort and edit like a spreadsheet.' },
    { icon: 'terminal', title: 'Write SQL', desc: 'Tabs, completion from your schema, history.' },
    { icon: 'sparkles', title: 'Ask AI', desc: 'Generate queries and fix errors with any model.' },
  ]

  const SYNC_POINTS = [
    { icon: 'key-round', title: 'Your license, on every machine' },
    { icon: 'lock', title: 'End-to-end encrypted sync' },
    { icon: 'bookmark', title: 'Connections and saved queries everywhere' },
  ]

  const KEYS = [
    { combo: 'Mod+K', label: 'Command palette' },
    { combo: 'Mod+P', label: 'Find a table' },
    { combo: 'Mod+B', label: 'Toggle sidebar' },
  ]

  const signedIn = $derived(!!$account?.signed_in)
  const user = $derived($account?.user ?? null)

  function next() { step = Math.min(step + 1, TOTAL) }
  function back() { step = Math.max(step - 1, 1) }

  function finish() {
    try { localStorage.setItem(KEY, '1') } catch {}
    open = false
  }
  function done(connect = false) {
    finish()
    if (connect) onconnect()
  }
  function trySample() {
    finish()
    onsample()
  }

  /** @param {KeyboardEvent} e */
  function onWindowKeydown(e) {
    if (!open) return
    if (e.key === 'Escape') { e.preventDefault(); done(false) }
    else if (e.key === 'Enter') { e.preventDefault(); step < TOTAL ? next() : done(true) }
    else if (e.key === 'ArrowRight' && step < TOTAL) next()
    else if (e.key === 'ArrowLeft' && step > 1) back()
  }

  // Keep the app's global hotkeys from firing behind the overlay; native
  // editing combos still work.
  $effect(() => {
    if (!open) return
    /** @param {KeyboardEvent} e */
    function blockAppHotkeys(e) {
      const mod = e.metaKey || e.ctrlKey
      if (!mod && !e.altKey) return
      const k = e.key.toLowerCase()
      if (mod && !e.altKey && ['c', 'v', 'x', 'a', 'z', 'y'].includes(k)) return
      e.stopImmediatePropagation()
    }
    window.addEventListener('keydown', blockAppHotkeys, { capture: true })
    return () => window.removeEventListener('keydown', blockAppHotkeys, { capture: true })
  })
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#if open}
  <div
    class="fixed inset-x-0 bottom-0 z-[200] flex flex-col bg-background"
    style={isMac ? 'top: 0;' : 'top: var(--app-titlebar-h, 38px);'}
    transition:fade={{ duration: 160 }}
  >
    <!-- Top row: drag region on macOS (traffic lights sit at its left), Skip at the right. -->
    <header class="flex h-[52px] shrink-0 items-center justify-end px-5" data-tauri-drag-region>
      <!-- Plain text, no filled hover: skipping is the quiet way out, not an
           action that should compete with the step's own button. -->
      <button
        type="button"
        class="group inline-flex h-7 items-center gap-2 rounded-md px-1.5 text-ui-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
        onclick={() => done(false)}
      >
        Skip for now
        <Kbd combo="Escape" size="sm" class="opacity-70 transition-opacity group-hover:opacity-100" />
      </button>
    </header>

    <div class="relative min-h-0 flex-1 overflow-hidden">
      {#key step}
        <div
          class="absolute inset-0 flex items-center justify-center overflow-y-auto px-6 pb-16"
          in:fade={{ duration: 160, easing: cubicOut }}
          out:fade={{ duration: 80 }}
        >
          <div class="flex w-full max-w-lg flex-col py-10">
            {#if step === 1}
              <Logo class="size-8" />
              <h1 class="mt-5 text-ui-2xl font-semibold tracking-tight text-foreground">Welcome to Stroke</h1>
              <p class="mt-1.5 text-ui-sm leading-relaxed text-muted-foreground">
                A fast, native database client. Connect, explore and query, with AI when you want it.
              </p>

              <ul class="mt-7 flex flex-col gap-1">
                {#each FEATURES as f (f.title)}
                  <li class="flex items-center gap-3.5 py-2">
                    <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-muted/50">
                      <Icon name={f.icon} class="size-4 shrink-0 text-foreground/80" />
                    </span>
                    <span class="min-w-0 flex-1">
                      <span class="block text-ui-sm font-medium text-foreground">{f.title}</span>
                      <span class="block text-ui-xs text-muted-foreground">{f.desc}</span>
                    </span>
                  </li>
                {/each}
              </ul>

              <!-- Every engine the connection dialog opens, as its mark only. -->
              <div class="mt-8 flex flex-wrap items-center gap-x-4 gap-y-3">
                {#each ENGINES as [id, name] (id)}
                  <span title={name} aria-label={name} class="inline-flex">
                    <DbIcon {id} class={cn('size-5 shrink-0', engineTint(id))} />
                  </span>
                {/each}
              </div>

              <div class="mt-9 flex items-center gap-2">
                <button
                  type="button"
                  class="inline-flex h-9 items-center gap-1.5 rounded-md bg-foreground px-4 text-ui-sm font-medium text-background transition-opacity hover:opacity-90"
                  onclick={next}
                >
                  Get started
                  <Icon name="arrow-right" class="size-3.5 shrink-0" />
                </button>
                <span class="ml-2 text-ui-xs text-muted-foreground">or press <Kbd combo="Enter" size="sm" class="mx-0.5" /></span>
              </div>
            {:else if step === 2}
              <span class="grid size-10 place-items-center rounded-lg border border-border/60 bg-muted/40">
                <Icon name={signedIn ? 'check' : 'shield-check'} class={cn('size-4.5 shrink-0', signedIn ? 'text-success' : 'text-foreground/80')} />
              </span>
              <h1 class="mt-5 text-ui-2xl font-semibold tracking-tight text-foreground">
                {signedIn ? "You're signed in" : 'Sign in to Stroke'}
              </h1>
              <p class="mt-1.5 text-ui-sm leading-relaxed text-muted-foreground text-pretty">
                {signedIn
                  ? 'Your license and sync are ready on this device.'
                  : 'Keep your license and sync connections and saved queries across devices.'}
              </p>

              {#if signedIn}
                <!-- `user` can be missing while the profile is still loading (or the
                     backend is local); the card still says who is signed in. -->
                <div class="mt-7 flex items-center gap-3 rounded-lg border border-border/60 bg-muted/30 px-4 py-3">
                  {#if user}
                    <AccountAvatar {user} class="size-8" />
                  {:else}
                    <span class="grid size-8 shrink-0 place-items-center rounded-full bg-muted">
                      <Icon name="key-round" class="size-4 shrink-0 text-muted-foreground" />
                    </span>
                  {/if}
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-ui-sm font-medium text-foreground">{user?.name || user?.email || 'Stroke account'}</span>
                    <span class="block truncate text-ui-xs text-muted-foreground">{user?.name && user?.email ? user.email : 'Signed in'}</span>
                  </span>
                </div>
              {:else}
                <ul class="mt-7 flex flex-col gap-2.5">
                  {#each SYNC_POINTS as p (p.title)}
                    <li class="flex items-center gap-3 text-ui-sm text-foreground/90">
                      <Icon name={p.icon} class="size-4 shrink-0 text-muted-foreground" />
                      {p.title}
                    </li>
                  {/each}
                </ul>
                <div class="mt-7">
                  <AccountSignInButtons onsignedin={next} />
                </div>
              {/if}

              <div class="mt-9 flex items-center gap-2">
                <Button variant="ghost" class="text-muted-foreground" onclick={back}>
                  <Icon name="arrow-left" class="size-3.5 shrink-0" />
                  Back
                </Button>
                <span class="flex-1"></span>
                {#if signedIn}
                  <Button onclick={next}>
                    Continue
                    <Icon name="arrow-right" class="size-3.5 shrink-0" />
                  </Button>
                {:else}
                  <Button variant="ghost" class="text-muted-foreground" onclick={next}>Not now</Button>
                {/if}
              </div>
            {:else}
              <h1 class="text-ui-2xl font-semibold tracking-tight text-foreground">Add your first database</h1>
              <p class="mt-1.5 text-ui-sm leading-relaxed text-muted-foreground">
                Paste a connection string, sign in to a provider, or start with sample data.
              </p>

              <div class="mt-8 flex flex-col gap-2">
                <button
                  type="button"
                  class="group flex items-center gap-3 rounded-md bg-muted/40 px-4 py-3.5 text-left transition-colors hover:bg-accent"
                  onclick={() => done(true)}
                >
                  <Icon name="plus" class="size-4 shrink-0 text-foreground" />
                  <span class="min-w-0 flex-1">
                    <span class="block text-ui-sm font-medium text-foreground">Connect a database</span>
                    <span class="block text-ui-xs text-muted-foreground">Postgres, MySQL, SQLite, D1 and more</span>
                  </span>
                  <Icon name="arrow-right" class="size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
                </button>
                <button
                  type="button"
                  class="group flex items-center gap-3 rounded-md px-4 py-3.5 text-left transition-colors hover:bg-muted/40"
                  onclick={trySample}
                >
                  <Icon name="flask-conical" class="size-4 shrink-0 text-muted-foreground" />
                  <span class="min-w-0 flex-1">
                    <span class="block text-ui-sm text-foreground">Try a sample database</span>
                    <span class="block text-ui-xs text-muted-foreground">A ready-made SQLite dataset to explore</span>
                  </span>
                  <Icon name="arrow-right" class="size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5" />
                </button>
              </div>

              <div class="mt-8 flex flex-col gap-1.5 px-1">
                <span class="text-ui-xs font-medium text-foreground/80">Handy keys</span>
                {#each KEYS as k (k.combo)}
                  <div class="flex items-center justify-between text-ui-xs text-muted-foreground">
                    <span>{k.label}</span>
                    <Kbd combo={k.combo} size="sm" />
                  </div>
                {/each}
              </div>

              <button
                type="button"
                class="mt-10 inline-flex h-8 w-fit items-center gap-1.5 rounded-md px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                onclick={back}
              >
                <Icon name="arrow-left" class="size-3.5 shrink-0" />
                Back
              </button>
            {/if}
          </div>
        </div>
      {/key}

      <!-- Progress: two small dots, bottom centre. -->
      <div class="pointer-events-none absolute inset-x-0 bottom-6 flex justify-center gap-1.5" aria-hidden="true">
        {#each Array(TOTAL) as _, i (i)}
          <span class="h-1.5 rounded-full transition-[width,background-color] duration-200 ease-[var(--ease-out)] {i + 1 === step ? 'w-4 bg-foreground/70' : 'w-1.5 bg-muted-foreground/30'}"></span>
        {/each}
      </div>
    </div>
  </div>
{/if}
