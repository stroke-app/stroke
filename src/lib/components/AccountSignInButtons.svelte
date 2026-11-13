<script>
  // "Continue with GitHub / Google", the waiting state with Cancel, and the
  // error state. Shared by the new-install screen and the account card.
  import { signInState, signIn, cancelSignIn } from '$lib/stores/account.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import AuthProviderIcon from './AuthProviderIcon.svelte'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import Copy from '@lucide/svelte/icons/copy'
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste'
  import PasteCodeForm from './PasteCodeForm.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** `lg` for the full-screen welcome, `sm` inside settings. */
    size = /** @type {'lg' | 'sm'} */ ('lg'),
    onsignedin = () => {},
    class: className = '',
  } = $props()

  const waiting = $derived($signInState.phase === 'waiting')
  /** The paste-a-code field is open (reset with each new sign-in). */
  let pasting = $state(false)
  $effect(() => {
    if (!waiting) pasting = false
  })
  const providerName = $derived($signInState.provider === 'google' ? 'Google' : $signInState.provider === 'github' ? 'GitHub' : 'your account')

  /** @param {'github' | 'google'} p */
  async function go(p) {
    if (await signIn(p)) onsignedin()
  }

  async function copyLink() {
    const url = $signInState.url
    if (!url) return
    try { await navigator.clipboard.writeText(url) } catch { /* clipboard denied */ }
  }
</script>

<div class={cn('flex w-full flex-col gap-2', className)}>
  {#if waiting}
    <!-- No box: a spinner and one line, one muted line, then quiet actions in
         a single row that wraps instead of spilling out. -->
    <div class="flex flex-col items-center gap-2 text-center" role="status" aria-live="polite">
      <div class="flex items-center gap-2 text-ui-sm text-foreground">
        <LoaderCircle class="size-4 shrink-0 animate-spin text-muted-foreground" />
        Waiting for {providerName}…
      </div>
      <p class="text-ui-xs text-muted-foreground text-pretty">Finish signing in in your browser, then come back here.</p>
      <div class="mt-2 flex flex-wrap items-center justify-center gap-x-1 gap-y-1 text-ui-xs">
        <button type="button" class="h-7 rounded-md px-2 text-muted-foreground hover:text-foreground" onclick={() => void cancelSignIn()}>Cancel</button>
        {#if $signInState.url}
          <span class="text-muted-foreground/50" aria-hidden="true">·</span>
          <button type="button" class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-muted-foreground hover:text-foreground" onclick={() => void copyLink()} title="Copy the sign-in link to open it in another browser">
            <Copy class="size-3.5 shrink-0" /> Copy link
          </button>
        {/if}
        {#if !pasting}
          <span class="text-muted-foreground/50" aria-hidden="true">·</span>
          <button type="button" class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-muted-foreground hover:text-foreground" onclick={() => (pasting = true)} title="The browser shows a code when it can't return to Stroke">
            <ClipboardPaste class="size-3.5 shrink-0" /> Paste code
          </button>
        {/if}
      </div>
      {#if pasting}
        <PasteCodeForm autofocus ondone={() => (pasting = false)} />
      {/if}
    </div>
  {:else}
    <Button
      variant="secondary"
      size={size === 'lg' ? 'lg' : 'sm'}
      class={cn('w-full gap-2.5 font-medium', size === 'lg' && 'text-ui-sm')}
      onclick={() => void go('github')}
    >
      <AuthProviderIcon name="github" class="size-4 shrink-0" />
      Continue with GitHub
    </Button>
    <Button
      variant="secondary"
      size={size === 'lg' ? 'lg' : 'sm'}
      class={cn('w-full gap-2.5 font-medium', size === 'lg' && 'text-ui-sm')}
      onclick={() => void go('google')}
    >
      <AuthProviderIcon name="google" class="size-4 shrink-0" />
      Continue with Google
    </Button>
    {#if $signInState.phase === 'error'}
      <p class="flex items-start gap-1.5 text-left text-ui-xs leading-relaxed text-destructive" role="alert">
        <CircleAlert class="mt-0.5 size-3.5 shrink-0" />
        {$signInState.error}
      </p>
    {/if}
  {/if}
</div>
