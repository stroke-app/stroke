<script>
  // Paste-a-code sign-in fallback (docs/accounts-and-sync.md, Appendix B): when
  // the browser can't redirect back to the app, the website offers
  // `stroke-code:<code>`; pasting it here finishes the sign-in in flight.
  import { Button } from '$lib/components/ui/button/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { pasteSignInCode, parseSignInCode } from '$lib/stores/account.js'

  let { ondone = () => {}, autofocus = false } = $props()

  let text = $state('')
  let busy = $state(false)
  let error = $state(/** @type {string | null} */ (null))
  const valid = $derived(!!parseSignInCode(text))

  /** @param {SubmitEvent} e */
  async function submit(e) {
    e.preventDefault()
    if (busy) return
    if (!valid) {
      error = "That doesn't look like a Stroke sign-in code."
      return
    }
    busy = true
    error = null
    const r = await pasteSignInCode(text)
    busy = false
    if (r.ok) {
      text = ''
      ondone()
    } else error = r.error ?? 'Sign-in failed.'
  }
</script>

<form class="flex flex-col gap-1.5" onsubmit={submit}>
  <div class="flex items-center gap-2">
    <!-- svelte-ignore a11y_autofocus -->
    <Input
      bind:value={text}
      placeholder="stroke-code:…"
      aria-label="Sign-in code"
      autocomplete="off"
      spellcheck={false}
      disabled={busy}
      {autofocus}
      aria-invalid={!!error}
      class="flex-1 font-mono"
    />
    <Button type="submit" size="sm" class="shrink-0" disabled={busy || !text.trim()}>{busy ? 'Signing in…' : 'Sign in'}</Button>
  </div>
  {#if error}<p class="text-left text-ui-xs leading-relaxed text-destructive" role="alert">{error}</p>{/if}
</form>
