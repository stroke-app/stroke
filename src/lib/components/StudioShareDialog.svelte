<script>
  /**
   * Labs: share the open database as a web studio. One link, opened in any
   * browser on any device, while Stroke is open. The app answers it through
   * the relay at stroke.click, end to end encrypted (studio-share.js).
   *
   * Read-only by default. Allowing edits makes a new link: an old read-only
   * link must never quietly start writing.
   */
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    /** The database's name, for the title. */
    databaseName = '',
    /** The running share (or null): { link, write }. */
    share = /** @type {{ link: string, write: boolean } | null} */ (null),
    /** 'connecting' | 'online' | 'offline' */
    status = 'offline',
    /** The engine isn't one the web studio speaks; the reason, in words. */
    unsupported = '',
    /** Start sharing, read-only or with edits. */
    onstart = /** @type {(write: boolean) => void} */ (() => {}),
    onstop = () => {},
  } = $props()

  let write = $state(false)
  let copied = $state(false)
  let busy = $state(false)

  $effect(() => {
    if (share) write = share.write
  })

  async function copy() {
    if (!share) return
    try {
      await navigator.clipboard.writeText(share.link)
      copied = true
      setTimeout(() => (copied = false), 1600)
    } catch { /* the link is selectable */ }
  }

  async function start() {
    busy = true
    try { await onstart(write) } finally { busy = false }
  }

  /** A different level is a different link. */
  async function setWrite(/** @type {boolean} */ v) {
    write = v
    if (share && share.write !== v) {
      onstop()
      await start()
    }
  }

  const statusText = $derived(
    status === 'online' ? 'Live' : status === 'connecting' ? 'Connecting' : 'Offline, reconnecting',
  )
  const hint = 'text-ui-2xs leading-relaxed text-muted-foreground'
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="w-[min(30rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0">
    <Dialog.Header class="flex-row items-center gap-3 px-5 pt-5 pb-4 pe-12 text-start">
      <span class={cn('flex size-9 shrink-0 items-center justify-center rounded-lg', share ? 'bg-success/12 text-success' : 'bg-muted/60 text-foreground')}>
        <Icon name={share ? 'radio' : 'globe'} class="size-4 shrink-0" />
      </span>
      <div class="flex min-w-0 flex-col gap-0.5">
        <Dialog.Title class="truncate text-ui-sm font-semibold">
          {share ? `Sharing ${databaseName || 'this database'}` : `Share ${databaseName || 'this database'} as a web studio`}
        </Dialog.Title>
        <Dialog.Description class="flex items-center gap-1 text-ui-2xs text-muted-foreground">
          <Icon name="lock" class="size-3 shrink-0" />
          End-to-end encrypted. stroke.click only passes it along.
        </Dialog.Description>
      </div>
    </Dialog.Header>

    <div class="flex flex-col gap-4 px-5 pb-5">
      {#if unsupported}
        <p class="text-ui-sm text-muted-foreground">{unsupported}</p>
      {:else}
        <!-- What the link can do. -->
        <div class="flex flex-col rounded-lg border border-border/60" role="radiogroup" aria-label="What the link can do">
          {#each [{ v: false, l: 'Read-only', d: 'Browse tables and run SELECT queries.' }, { v: true, l: 'Allow edits', d: 'Edit rows and run any SQL on this database.' }] as opt (opt.l)}
            <button
              type="button"
              role="radio"
              aria-checked={write === opt.v}
              disabled={busy}
              onclick={() => void setWrite(opt.v)}
              class="flex items-start gap-3 border-b border-border/40 px-3.5 py-3 text-start last:border-0 hover:bg-muted/30 disabled:opacity-60"
            >
              <span class={cn('mt-0.5 flex size-3.5 shrink-0 items-center justify-center rounded-full border', write === opt.v ? 'border-primary' : 'border-border')}>
                {#if write === opt.v}<span class="size-1.5 rounded-full bg-primary"></span>{/if}
              </span>
              <span class="min-w-0">
                <span class="block text-ui-xs font-medium text-foreground">{opt.l}</span>
                <span class={hint}>{opt.d}</span>
              </span>
            </button>
          {/each}
        </div>

        {#if share}
          <div class="flex h-10 items-center gap-2 rounded-lg border border-border/60 bg-muted/30 ps-3 pe-1">
            <Icon name="link-2" class="size-3.5 shrink-0 text-muted-foreground" />
            <button type="button" class="min-w-0 flex-1 truncate text-start font-mono text-ui-xs text-foreground select-all" title={share.link} onclick={copy}>{share.link}</button>
            <Button size="sm" variant={copied ? 'outline' : 'default'} class="h-8 w-[5.5rem] shrink-0 gap-1.5" onclick={copy}>
              <Icon name={copied ? 'check' : 'copy'} class="size-3.5 shrink-0" />{copied ? 'Copied' : 'Copy'}
            </Button>
          </div>
          <p class="flex items-center gap-1.5 text-ui-2xs text-muted-foreground" role="status" aria-live="polite">
            <span class={cn('size-1.5 shrink-0 rounded-full', status === 'online' ? 'bg-success' : status === 'connecting' ? 'bg-warning' : 'bg-muted-foreground')}></span>
            {statusText}. Anyone with the link can {share.write ? 'read and change' : 'read'} this database while Stroke is open.
          </p>
        {:else}
          <p class={hint}>
            You get a link that opens this database in any browser, on any device. It works while Stroke is open and this database is connected. Send the whole link: the key is the part after <span class="font-mono">#</span>.
          </p>
        {/if}
      {/if}
    </div>

    <div class="flex items-center gap-2 border-t border-border/40 px-5 py-3">
      <span class="text-ui-2xs text-muted-foreground">Labs</span>
      <span class="flex-1"></span>
      {#if share}
        <Button variant="outline" size="sm" class="gap-1.5 text-destructive hover:text-destructive" onclick={() => onstop()}>
          <Icon name="square" class="size-3.5 shrink-0" />Stop sharing
        </Button>
        <Button size="sm" onclick={() => (open = false)}>Done</Button>
      {:else if !unsupported}
        <Button variant="ghost" size="sm" onclick={() => (open = false)}>Cancel</Button>
        <Button size="sm" class="gap-1.5" disabled={busy} onclick={() => void start()}>
          <Icon name={busy ? 'loader-2' : 'link-2'} class={cn('size-3.5 shrink-0', busy && 'animate-spin')} />Create link
        </Button>
      {:else}
        <Button size="sm" onclick={() => (open = false)}>Close</Button>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
