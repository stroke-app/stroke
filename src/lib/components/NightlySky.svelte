<script>
  /**
   * Stroke Nightly's title bar: a night sky behind it, so Nightly is told
   * apart from Stable at a glance, even side by side. A violet glow from the
   * left edge, a scatter of stars that twinkle out of step, and every so often
   * one shooting star across the left of the bar. Painted under the bar's
   * content, takes no clicks, and holds still with reduced motion.
   */
  import { isNightly } from '$lib/build-track.js'

  // Fixed, hand-placed: x and y in % of the bar, size in px, twinkle delay in s.
  // Thick on the left where the glow is, thinning out towards the middle.
  const STARS = [
    [3, 30, 1.5, 0], [7, 70, 1, 1.7], [11, 22, 1, 3.1], [14, 58, 2, 0.6],
    [18, 36, 1, 2.4], [22, 78, 1.5, 4.2], [26, 18, 1, 1.1], [30, 52, 1, 3.6],
    [35, 28, 1.5, 2.0], [40, 66, 1, 0.3], [46, 40, 1, 4.8], [53, 20, 1, 2.8],
    [61, 72, 1, 1.4], [72, 34, 1, 3.9],
  ]
</script>

{#if $isNightly}
  <div class="nightly-sky pointer-events-none absolute inset-0 -z-10 overflow-hidden" aria-hidden="true">
    {#each STARS as [x, y, size, delay]}
      <span
        class="nightly-star absolute rounded-full"
        style:left="{x}%"
        style:top="{y}%"
        style:width="{size}px"
        style:height="{size}px"
        style:animation-delay="{delay}s"
      ></span>
    {/each}
    <span class="nightly-shooting-star absolute"></span>
  </div>
{/if}
