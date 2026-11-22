<script>
  /**
   * Settings > Network: a proxy for each kind of traffic (src-tauri/src/proxy.rs).
   * System follows the environment (HTTPS_PROXY and friends), Custom sends the
   * traffic through a proxy URL, Off always connects directly. Databases and SSH
   * are not HTTP, so they have no System setting of their own: Direct or Custom.
   */
  import { Button } from '$lib/components/ui/button/index.js'
  import Loader from '@lucide/svelte/icons/loader-2'
  import { cn } from '$lib/utils.js'
  import { loadSettings, updateSettings, normalizeProxy } from '$lib/stores/settings.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

  const initial = normalizeProxy(loadSettings().proxy)
  let proxy = $state(initial)

  const KINDS = /** @type {const} */ ([
    { id: 'app', label: 'App requests', desc: 'Update checks and downloads, sign-in, sync, sharing and provider sign-ins.', system: true },
    { id: 'ai', label: 'AI providers', desc: 'Requests to AI models, Copilot and the agent\'s web search.', system: true },
    { id: 'database', label: 'Database connections', desc: 'Every connection without a tunnel of its own. One with SSH, Kubernetes or SSM uses that instead.', system: false },
    { id: 'ssh', label: 'Tunnels', desc: 'The connection to an SSH bastion; kubectl and the AWS CLI get it as HTTPS_PROXY.', system: false },
  ])

  /** URL being typed per kind, saved on blur or Enter. */
  let drafts = $state(/** @type {Record<string, string>} */ ({ app: initial.app.url, ai: initial.ai.url, database: initial.database.url, ssh: initial.ssh.url }))
  let errors = $state(/** @type {Record<string, string>} */ ({}))
  let testing = $state(/** @type {Record<string, boolean>} */ ({}))

  /** @param {string} url */
  function check(url) {
    const u = url.trim()
    if (!u) return 'Enter the proxy, e.g. http://proxy.local:3128 or socks5://127.0.0.1:1080.'
    const m = /^(http|socks5h?):\/\/(?:[^@/]+@)?[^:/]+:(\d+)\/?$/i.exec(u)
    if (!m) return /^https:/i.test(u) ? 'HTTPS proxies are not supported: use http://host:port (what goes through it is still encrypted).' : 'Use http://host:port or socks5://host:port, with user:password@ if it needs a login.'
    return ''
  }

  /** @param {'app' | 'ai' | 'database' | 'ssh'} kind @param {'system' | 'custom' | 'off'} mode */
  function setMode(kind, mode) {
    // Custom shows its URL field at once; until the URL is valid, save() keeps
    // the previous route in force.
    errors[kind] = mode === 'custom' && drafts[kind].trim() ? check(drafts[kind]) : ''
    proxy = { ...proxy, [kind]: { mode, url: drafts[kind].trim() } }
    save()
  }

  /** @param {'app' | 'ai' | 'database' | 'ssh'} kind */
  function commitUrl(kind) {
    errors[kind] = check(drafts[kind])
    if (errors[kind]) return
    proxy = { ...proxy, [kind]: { mode: 'custom', url: drafts[kind].trim() } }
    save()
  }

  function save() {
    // Kinds left on Custom with no valid URL go back to their default until fixed.
    const clean = normalizeProxy(proxy)
    for (const k of /** @type {const} */ (['app', 'ai', 'database', 'ssh'])) {
      if (clean[k].mode === 'custom' && check(clean[k].url)) clean[k] = { mode: 'system', url: clean[k].url }
    }
    updateSettings({ proxy: clean })
  }

  /** @param {'app' | 'ai' | 'database' | 'ssh'} kind */
  async function test(kind) {
    errors[kind] = check(drafts[kind])
    if (errors[kind]) return
    testing[kind] = true
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const ms = await invoke('proxy_test', { url: drafts[kind].trim(), target: null })
      toast.success('The proxy works', { description: `Reached example.com:443 through it in ${ms} ms.` })
    } catch (e) {
      toast.error('The proxy did not work', { description: String(e) })
    } finally {
      testing[kind] = false
    }
  }

  const seg = 'h-7 rounded-md px-2.5 text-ui-xs font-medium transition-colors'
</script>

<div class="flex flex-col">
  <p class="pb-3 text-ui-sm text-pretty text-muted-foreground">
    Send each kind of traffic through a proxy, or straight out. A proxy is <code class="font-mono text-ui-xs">http://host:port</code> or <code class="font-mono text-ui-xs">socks5://host:port</code>, with <code class="font-mono text-ui-xs">user:password@</code> when it needs a login. New connections use the change; open ones keep their route until they reconnect.
  </p>
  {#each KINDS as k (k.id)}
    {@const mode = proxy[k.id].mode === 'system' && !k.system ? 'off' : proxy[k.id].mode}
    <div class="flex flex-col gap-2 border-t border-border/25 py-3.5">
      <div class="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
        <div class="min-w-0 flex-1">
          <p class="text-ui-sm font-medium text-foreground">{k.label}</p>
          <p class="mt-0.5 text-ui-xs leading-relaxed text-pretty text-muted-foreground">{k.desc}</p>
        </div>
        <div class="flex shrink-0 gap-0.5 rounded-lg bg-muted/40 p-0.5" role="radiogroup" aria-label="{k.label} proxy">
          {#if k.system}
            <button type="button" role="radio" aria-checked={mode === 'system'} class={cn(seg, mode === 'system' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground')} onclick={() => setMode(k.id, 'system')}>System</button>
          {/if}
          <button type="button" role="radio" aria-checked={mode === 'custom'} class={cn(seg, mode === 'custom' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground')} onclick={() => setMode(k.id, 'custom')}>Custom</button>
          <button type="button" role="radio" aria-checked={mode === 'off'} class={cn(seg, mode === 'off' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground')} onclick={() => setMode(k.id, 'off')}>{k.system ? 'Off' : 'Direct'}</button>
        </div>
      </div>
      {#if mode === 'custom'}
        <div class="flex items-center gap-2">
          <input
            class={cn('h-8 min-w-0 flex-1 rounded-md border bg-background px-2.5 font-mono text-ui-xs text-foreground outline-none placeholder:font-sans placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/30', errors[k.id] ? 'border-destructive/60' : 'border-border')}
            placeholder="socks5://127.0.0.1:1080"
            spellcheck="false"
            autocomplete="off"
            aria-label="{k.label} proxy URL"
            aria-invalid={!!errors[k.id]}
            bind:value={drafts[k.id]}
            onblur={() => commitUrl(k.id)}
            onkeydown={(e) => { if (e.key === 'Enter') commitUrl(k.id) }}
          />
          <Button variant="outline" size="sm" disabled={testing[k.id]} onclick={() => test(k.id)}>
            {#if testing[k.id]}<Loader class="size-3.5 shrink-0 animate-spin" />{/if}Test
          </Button>
        </div>
        {#if errors[k.id]}<p class="text-ui-2xs text-destructive">{errors[k.id]}</p>{/if}
      {/if}
    </div>
  {/each}
</div>
