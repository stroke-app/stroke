<script>
  import Server    from "@lucide/svelte/icons/server";
  import Copy      from "@lucide/svelte/icons/copy";
  import Check     from "@lucide/svelte/icons/check";
  import Power     from "@lucide/svelte/icons/power";
  import PowerOff  from "@lucide/svelte/icons/power-off";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import ShieldCheck  from "@lucide/svelte/icons/shield-check";
  import Bot       from "@lucide/svelte/icons/bot";
  import Code2     from "@lucide/svelte/icons/code-2";
  import Wand2     from "@lucide/svelte/icons/wand-2";
  import Terminal  from "@lucide/svelte/icons/terminal";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { cn } from "$lib/utils.js";
  import { mcpStart, mcpStop, mcpStatus, mcpSetReadonly } from "$lib/api.js";

  let { open = $bindable(false), connected = false } = $props();

  /** @type {{ running: boolean, port: number, url: string, token: string } | null} */
  let status = $state(null);
  let toggling = $state(false);
  /** @type {string | null} */
  let copied = $state(null);

  // Read-only mode - persisted to localStorage
  let readOnly = $state(
    typeof localStorage !== 'undefined'
      ? localStorage.getItem('stroke:mcp-readonly') === 'true'
      : false
  );

  $effect(() => {
    if (open) void refresh();
  });

  async function refresh() {
    try { status = await mcpStatus() } catch { status = null }
  }

  async function toggle() {
    toggling = true;
    try {
      if (status?.running) await mcpStop(); else await mcpStart();
      status = await mcpStatus();
    } catch (e) { console.error(e) }
    finally { toggling = false }
  }

  async function toggleReadOnly() {
    readOnly = !readOnly;
    if (typeof localStorage !== 'undefined')
      localStorage.setItem('stroke:mcp-readonly', String(readOnly));
    try { await mcpSetReadonly(readOnly) } catch {}
  }

  // Sync read-only setting to backend when the dialog opens
  $effect(() => {
    if (open) mcpSetReadonly(readOnly).catch(() => {})
  })

  const claudeConfig = $derived(status ? JSON.stringify({ mcpServers: { "stroke": { url: status.url, headers: { Authorization: `Bearer ${status.token}` } } } }, null, 2) : '')
  const cursorConfig = $derived(claudeConfig)
  // Claude Code registers remote servers from the CLI, so its "config" is a
  // one-line command rather than JSON to paste.
  const claudeCodeCommand = $derived(status ? `claude mcp add --transport http stroke ${status.url} --header "Authorization: Bearer ${status.token}"` : '')
  const vscodeConfig = $derived(status ? JSON.stringify({ servers: { "stroke": { type: "http", url: status.url, headers: { Authorization: `Bearer ${status.token}` } } } }, null, 2) : '')

  const cursorInstallUrl = $derived.by(() => {
    if (!status) return ''
    return `cursor://anysphere.cursor-deeplink/mcp/install?name=stroke&config=${btoa(JSON.stringify({ url: status.url, headers: { Authorization: `Bearer ${status.token}` } }))}`
  })
  const vscodeInstallUrl = $derived.by(() => {
    if (!status) return ''
    return `vscode:mcp/install?${encodeURIComponent(JSON.stringify({ name: 'stroke', type: 'http', url: status.url, headers: { Authorization: `Bearer ${status.token}` } }))}`
  })
  const vscodeInsidersUrl = $derived.by(() => {
    if (!status) return ''
    return vscodeInstallUrl.replace('vscode:', 'vscode-insiders:')
  })

  async function installVia(url) {
    try { const { openUrl } = await import('@tauri-apps/plugin-opener'); await openUrl(url) }
    catch {}
  }

  async function copy(text, key) {
    if (!text) return
    await navigator.clipboard.writeText(text)
    copied = key
    setTimeout(() => { copied = null }, 2000)
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-full max-w-xl gap-0 overflow-hidden rounded-2xl border-border/60 p-0 sm:max-w-xl">

    <!-- ── Header ── -->
    <div class="flex items-center gap-3 border-b border-border/40 px-6 py-5 pr-12">
      <div class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted/50 ring-1 ring-border/30">
        <Server class="size-4 shrink-0 text-foreground/70" />
      </div>
      <div class="flex min-w-0 flex-col gap-1">
        <Dialog.Title class="text-ui-xl font-semibold leading-none tracking-tight text-foreground">
          MCP Server
        </Dialog.Title>
        <Dialog.Description class="text-ui-sm leading-snug text-muted-foreground">
          Connect Claude, Cursor, or VS Code to your database.
        </Dialog.Description>
      </div>
    </div>

    <!-- ── Scrollable body ── -->
    <div class="flex flex-col overflow-y-auto" style="max-height: 70vh">

      <!-- Server: address and state live in one field; the button beside it
           changes that state. -->
      <div class="flex items-center gap-2 border-b border-border/30 px-6 py-4">
        {#if status}
          <div
            class={cn(
              "flex h-9 min-w-0 flex-1 items-center gap-2.5 rounded-lg border border-border/40 bg-muted/20 pl-3 pr-1",
              !status.running && "text-muted-foreground"
            )}
          >
            <span
              class={cn("size-1.5 shrink-0 rounded-full", status.running ? "bg-success" : "bg-muted-foreground/40")}
              aria-hidden="true"
            ></span>
            <code class={cn("min-w-0 flex-1 truncate font-mono text-ui-xs", status.running ? "text-foreground/85" : "text-muted-foreground")}>{status.url}</code>
            <span class={cn("shrink-0 text-ui-2xs font-medium", status.running ? "text-success" : "text-muted-foreground")}>
              {status.running ? 'Running' : 'Stopped'}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              class="text-muted-foreground hover:text-foreground"
              onclick={() => void copy(status?.url ?? '', 'url')}
              aria-label="Copy URL"
              title="Copy URL"
            >
              {#if copied === 'url'}<Check class="size-3.5 shrink-0 text-success" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
            </Button>
          </div>
        {:else}
          <div class="flex h-9 flex-1 items-center rounded-lg border border-dashed border-border/40 px-3 text-ui-xs text-muted-foreground">
            {connected ? 'Loading server status' : 'No database connected'}
          </div>
        {/if}

        <Button
          type="button"
          variant={status?.running ? 'ghost' : 'default'}
          size="lg"
          class={cn("w-24 shrink-0", status?.running && "bg-muted/50 text-foreground hover:bg-destructive/15 hover:text-destructive dark:hover:bg-destructive/15")}
          disabled={toggling || !connected || !status}
          onclick={() => void toggle()}
        >
          {#if toggling}
            <span class="size-3 shrink-0 animate-spin rounded-full border-2 border-current border-t-transparent"></span>
            {status?.running ? 'Stopping' : 'Starting'}
          {:else if status?.running}
            <PowerOff class="size-3.5 shrink-0" data-icon="inline-start" />
            Stop
          {:else}
            <Power class="size-3.5 shrink-0" data-icon="inline-start" />
            Start
          {/if}
        </Button>
      </div>

      <!-- ── Read-only mode ── -->
      <div class="border-b border-border/30 px-6 py-4">
        <button
          type="button"
          role="switch"
          aria-checked={readOnly}
          class="flex w-full items-center gap-3 text-left"
          onclick={toggleReadOnly}
        >
          <div class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/40 ring-1 ring-border/25">
            <ShieldCheck class={cn("size-4 shrink-0", readOnly ? "text-warning" : "text-muted-foreground")} />
          </div>
          <div class="flex min-w-0 flex-1 flex-col gap-0.5">
            <span class="text-ui-sm font-medium text-foreground">Read-only mode</span>
            <span class="text-ui-xs leading-snug text-muted-foreground">
              {readOnly
                ? 'Only SELECT queries are permitted. Write operations are blocked.'
                : 'All SQL operations are allowed. Enable to restrict the agent to reads only.'}
            </span>
          </div>
          <span
            class={cn(
              "flex h-5 w-9 shrink-0 items-center rounded-full px-0.5 ring-1 ring-inset transition-colors duration-200",
              readOnly ? "bg-warning/20 ring-warning/40" : "bg-muted/30 ring-border/40"
            )}
            aria-hidden="true"
          >
            <span
              class={cn(
                "size-4 rounded-full transition-transform duration-200",
                readOnly ? "translate-x-4 bg-warning" : "translate-x-0 bg-muted-foreground/40"
              )}
            ></span>
          </span>
        </button>
      </div>

      <!-- ── Clients ── one grouped list, every row ends in the same actions:
           Copy config always, plus a one-click Add where the client has an
           install link. -->
      {#if status}
        <div class="flex flex-col gap-3 px-6 py-5">
          <div class="flex flex-col gap-0.5">
            <p class="text-ui-sm font-medium text-foreground">Connect a client</p>
            <p class="text-ui-xs text-muted-foreground">
              {status.running ? 'The config includes this session\'s access token.' : 'Start the server first. The config works once it is running.'}
            </p>
          </div>

          <div class="flex flex-col divide-y divide-border/40 overflow-hidden rounded-lg border border-border/40">
            <!-- Soft filled buttons: the outline variant's field border is
                 the input token and reads as a stack of text boxes here. -->
            {#snippet softButton(/** @type {() => void} */ onclick, /** @type {any} */ Icon, /** @type {string} */ label, /** @type {string} */ doneLabel, /** @type {boolean} */ done)}
              <Button
                type="button"
                variant="ghost"
                size="sm"
                class="bg-muted/50 text-foreground/85 hover:bg-muted hover:text-foreground dark:hover:bg-muted"
                {onclick}
              >
                {#if done}<Check class="size-3.5 shrink-0 text-success" data-icon="inline-start" />{:else}<Icon class="size-3.5 shrink-0" data-icon="inline-start" />{/if}
                <!-- Both labels share one grid cell, so "Copied" never changes the width. -->
                <span class="grid">
                  <span class={cn("col-start-1 row-start-1", done && "invisible")}>{label}</span>
                  <span class={cn("col-start-1 row-start-1", !done && "invisible")} aria-hidden={!done}>{doneLabel}</span>
                </span>
              </Button>
            {/snippet}

            {#snippet clientRow(/** @type {any} */ Icon, /** @type {string} */ name, /** @type {import('svelte').Snippet} */ hint, /** @type {string} */ copyKey, /** @type {string} */ copyText, /** @type {string} */ copyLabel, /** @type {string} */ installUrl)}
              <div class="flex items-center gap-3 px-3.5 py-3">
                <div class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/50 ring-1 ring-border/30">
                  <Icon class="size-4 shrink-0 text-foreground/70" />
                </div>
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span class="truncate text-ui-sm font-medium text-foreground">{name}</span>
                  <span class="truncate text-ui-2xs text-muted-foreground">{@render hint()}</span>
                </div>
                <div class="flex shrink-0 items-center gap-1.5">
                  {@render softButton(() => void copy(copyText, copyKey), Copy, copyLabel, 'Copied', copied === copyKey)}
                  {#if installUrl}
                    {@render softButton(() => void installVia(installUrl), ExternalLink, 'Add', 'Add', false)}
                  {/if}
                </div>
              </div>
            {/snippet}

            {#snippet desktopHint()}<code class="font-mono">claude_desktop_config.json</code>{/snippet}
            {#snippet codeHint()}Terminal, <code class="font-mono">claude mcp add</code>{/snippet}
            {#snippet cursorHint()}Copy the config or add in one click{/snippet}
            {#snippet vscodeHint()}
              Add in one click, or
              <button
                type="button"
                class="font-medium text-foreground/80 underline decoration-border underline-offset-2 transition-colors hover:text-foreground hover:decoration-foreground/50"
                onclick={() => void installVia(vscodeInsidersUrl)}
              >Insiders</button>
            {/snippet}

            {@render clientRow(Bot, 'Claude Desktop', desktopHint, 'claude', claudeConfig, 'Copy config', '')}
            {@render clientRow(Terminal, 'Claude Code', codeHint, 'claude-code', claudeCodeCommand, 'Copy command', '')}
            {@render clientRow(Wand2, 'Cursor', cursorHint, 'cursor', cursorConfig, 'Copy config', cursorInstallUrl)}
            {@render clientRow(Code2, 'VS Code', vscodeHint, 'vscode', vscodeConfig, 'Copy config', vscodeInstallUrl)}
          </div>
        </div>
      {:else if connected}
        <div class="flex flex-col items-center justify-center gap-3 px-6 py-14 text-center">
          <div class="flex size-12 items-center justify-center rounded-lg border border-border/30 bg-muted/20">
            <Server class="size-5 shrink-0 text-muted-foreground" />
          </div>
          <p class="text-ui-sm text-muted-foreground">Loading server status</p>
        </div>
      {:else}
        <div class="flex flex-col items-center justify-center gap-3 px-6 py-14 text-center">
          <div class="flex size-12 items-center justify-center rounded-lg border border-border/30 bg-muted/20">
            <Server class="size-5 shrink-0 text-muted-foreground" />
          </div>
          <p class="max-w-[220px] text-ui-sm text-muted-foreground">
            Connect to a database first, then start the MCP server.
          </p>
        </div>
      {/if}

    </div>
  </Dialog.Content>
</Dialog.Root>
