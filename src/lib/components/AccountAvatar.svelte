<script>
  // The account's picture, or its initials when there is none (or it fails to load).
  import { initials } from '$lib/stores/account.js'
  import { cn } from '$lib/utils.js'

  /** @type {{ user: import('$lib/stores/account.js').AccountUser | null | undefined, class?: string }} */
  let { user, class: className = '' } = $props()

  let broken = $state(false)
  $effect(() => {
    void user?.image
    broken = false
  })
</script>

{#if user?.image && !broken}
  <img
    src={user.image}
    alt=""
    referrerpolicy="no-referrer"
    class={cn('size-5 shrink-0 rounded-full object-cover', className)}
    onerror={() => (broken = true)}
  />
{:else}
  <span
    class={cn('grid size-5 shrink-0 place-items-center rounded-full bg-foreground/[0.08] text-ui-3xs font-semibold text-foreground', className)}
    aria-hidden="true"
  >
    {initials(user)}
  </span>
{/if}
