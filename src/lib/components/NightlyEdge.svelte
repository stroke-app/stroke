<script>
  /**
   * A 1px line across the top of the window in Stroke Nightly, so it is told
   * apart from Stable at a glance even side by side. Takes no room and no clicks.
   */
  import { isNightly } from '$lib/build-track.js'
</script>

{#if $isNightly}
  <div class="nightly-edge pointer-events-none fixed inset-x-0 top-0 z-[200] h-px" aria-hidden="true"></div>
{/if}
