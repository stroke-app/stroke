<script>
  // The re-auth prompt for a user-initiated connect while the app is already
  // open. Driven entirely by the `pinChallenge` store, so any caller can await
  // `requireUnlock()` without owning a dialog.
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'
  import PinField from './PinField.svelte'
  import { pinChallenge, answerChallenge, verifyPin, lockStatus } from '$lib/stores/app-lock.js'

  let pin = $state('')
  let checking = $state(false)
  let wrong = $state(false)

  const open = $derived(!!$pinChallenge)

  // Reset per opening, not per keystroke.
  $effect(() => {
    if ($pinChallenge) {
      pin = ''
      wrong = false
      checking = false
    }
  })

  /** @param {string} entered */
  async function submit(entered) {
    if (checking || entered.length !== $lockStatus.pinLength) return
    checking = true
    wrong = false
    const ok = await verifyPin(entered)
    checking = false
    if (ok) {
      answerChallenge(true)
      return
    }
    wrong = true
    pin = ''
    setTimeout(() => (wrong = false), 900)
  }

  function cancel() {
    answerChallenge(false)
  }
</script>

<Dialog.Root {open} onOpenChange={(next) => { if (!next) cancel() }}>
  <Dialog.Portal>
    <Dialog.Overlay />
    <Dialog.Content
      showCloseButton={false}
      class="w-[min(20rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0 sm:max-w-none"
    >
      <div class="flex flex-col items-center px-6 pt-6 pb-5 text-center">
        <div class="grid size-9 place-items-center rounded-lg border border-border/50 bg-muted/30">
          <Icon name="lock" class="size-4 shrink-0 text-muted-foreground" />
        </div>

        <Dialog.Title class="mt-3.5 text-ui-sm font-semibold text-foreground">Confirm your PIN</Dialog.Title>
        <Dialog.Description class="mt-1 text-ui-xs text-balance text-muted-foreground">
          {$pinChallenge?.reason ?? 'Unlock to continue'}
        </Dialog.Description>

        <div class="mt-5">
          <PinField
            bind:value={pin}
            length={$lockStatus.pinLength}
            disabled={checking}
            invalid={wrong}
            autofocus
            onsubmit={submit}
            aria-label="App PIN"
          />
        </div>

        <p class="mt-2 h-4 text-ui-2xs {wrong ? 'text-destructive' : 'text-muted-foreground'}">
          {#if checking}Checking…{:else if wrong}Wrong PIN. Try again.{/if}
        </p>

        <button
          type="button"
          class="mt-2 inline-flex h-7 items-center rounded-md px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
          onclick={cancel}>Cancel</button
        >
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
