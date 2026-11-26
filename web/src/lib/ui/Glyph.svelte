<script lang="ts" module>
  export type GlyphName = 'copy' | 'check' | 'refresh';
</script>

<script lang="ts">
  /**
   * A 16px symbol that moves like an SF Symbol (D619): `check` is written on as it appears, so
   * `copy` swapping to `check` reads as "copied"; `refresh` is the one `play(el, 'rotate')` spins.
   */
  let { name }: { name: GlyphName } = $props();
</script>

<svg class="st-icon vc-glyph" viewBox="0 0 24 24" aria-hidden="true">
  {#if name === 'check'}
    <path class="vc-sym-write" pathLength="1" d="m5 12.5 4.5 4.5L19 7.5" />
  {:else if name === 'copy'}
    <path d="M10 8h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-8a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z" />
    <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
  {:else}
    <path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3" />
    <path d="M20 4v4h-4" />
  {/if}
</svg>

<style>
  .vc-glyph {
    width: var(--st-size-icon-sm);
    height: var(--st-size-icon-sm);
  }
</style>
