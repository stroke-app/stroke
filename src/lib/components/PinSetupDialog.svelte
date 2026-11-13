<script>
  // Set, change, or remove the app PIN. All three are "type a PIN, maybe twice"
  // so they share one dialog and differ only in which steps run.
  import { untrack } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import Icon from './Icon.svelte'
  import PinField from './PinField.svelte'
  import { lockStatus, setPin, disablePin } from '$lib/stores/app-lock.js'

  let {
    open = $bindable(false),
    /** 'set' when no PIN exists yet, 'change' to replace one, 'remove' to drop it. */
    mode = 'set',
  } = $props()

  /** @type {'current' | 'new' | 'confirm'} */
  let step = $state('current')
  let current = $state('')
  let next = $state('')
  let confirm = $state('')
  let busy = $state(false)
  let error = $state('')

  const len = $derived($lockStatus.pinLength)
  const isRemove = $derived(mode === 'remove')
  const needsCurrent = $derived(mode !== 'set')

  const title = $derived(
    isRemove ? 'Remove PIN' : mode === 'change' ? 'Change PIN' : 'Set a PIN',
  )
  const prompt = $derived(
    step === 'current'
      ? isRemove
        ? 'Enter your current PIN to turn the lock off'
        : 'Enter your current PIN'
      : step === 'new'
        ? `Choose a ${len}-digit PIN`
        : 'Enter it once more',
  )

  let wasOpen = false
  $effect(() => {
    if (!open) {
      wasOpen = false
      return
    }
    if (wasOpen) return
    wasOpen = true
    untrack(() => {
      step = needsCurrent ? 'current' : 'new'
      current = ''
      next = ''
      confirm = ''
      error = ''
      busy = false
    })
  })

  /** @param {string} entered */
  async function submit(entered) {
    if (busy || entered.length !== len) return
    error = ''

    if (step === 'current') {
      current = entered
      if (isRemove) return void remove()
      step = 'new'
      next = ''
      return
    }

    if (step === 'new') {
      next = entered
      step = 'confirm'
      confirm = ''
      return
    }

    if (entered !== next) {
      error = "Those didn't match. Start again."
      step = 'new'
      next = ''
      confirm = ''
      return
    }
    await save()
  }

  async function save() {
    busy = true
    try {
      await setPin(next, needsCurrent ? current : undefined)
      open = false
    } catch (e) {
      error = String(e).replace(/^Error:\s*/i, '')
      step = needsCurrent ? 'current' : 'new'
      current = ''
      next = ''
      confirm = ''
    } finally {
      busy = false
    }
  }

  async function remove() {
    busy = true
    try {
      await disablePin(current)
      open = false
    } catch (e) {
      error = String(e).replace(/^Error:\s*/i, '')
      current = ''
    } finally {
      busy = false
    }
  }

  function back() {
    error = ''
    if (step === 'confirm') {
      step = 'new'
      next = ''
      confirm = ''
    } else if (step === 'new' && needsCurrent) {
      step = 'current'
      current = ''
    } else {
      open = false
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay />
    <Dialog.Content showCloseButton={false} class="w-[min(21.25rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0 sm:max-w-none">
      <div class="flex flex-col items-center px-6 pt-6 pb-5 text-center">
        <div class="grid size-9 place-items-center rounded-lg border border-border/50 bg-muted/30">
          <Icon name={isRemove ? 'lock-open' : 'lock'} class="size-4 shrink-0 text-muted-foreground" />
        </div>

        <Dialog.Title class="mt-3.5 text-ui-sm font-semibold text-foreground">{title}</Dialog.Title>
        <Dialog.Description class="mt-1 text-ui-xs text-balance text-muted-foreground">{prompt}</Dialog.Description>

        <div class="mt-5">
          {#key step}
            <PinField
              value={step === 'current' ? current : step === 'new' ? next : confirm}
              length={len}
              disabled={busy}
              invalid={!!error}
              autofocus
              onsubmit={submit}
              aria-label={prompt}
            />
          {/key}
        </div>

        <p class="mt-2 h-4 text-ui-2xs {error ? 'text-destructive' : 'text-muted-foreground'}">
          {#if busy}Saving…{:else if error}{error}{/if}
        </p>

        {#if step === 'new' && !isRemove}
          <p class="mt-2 text-ui-2xs leading-relaxed text-balance text-muted-foreground">
            Kept as a salted hash in your OS keychain, never as the digits. There is
            no reset link, so pick one you will remember.
          </p>
        {/if}
      </div>

      <div class="flex items-center justify-between border-t border-border/25 px-4 py-2.5">
        <button
          type="button"
          class="inline-flex h-7 items-center rounded-md px-2.5 text-ui-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
          onclick={back}>{step === 'current' || (step === 'new' && !needsCurrent) ? 'Cancel' : 'Back'}</button
        >
        <p class="text-ui-3xs text-muted-foreground">
          {#if !isRemove}Step {step === 'current' ? 1 : step === 'new' ? (needsCurrent ? 2 : 1) : needsCurrent ? 3 : 2} of {needsCurrent ? 3 : 2}{/if}
        </p>
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
