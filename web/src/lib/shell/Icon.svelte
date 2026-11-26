<script lang="ts">
  import type { IconName } from './spaces.ts';

  /** The shell's 24px stroke icons, from design/mockups/ui-v2/index.html's symbols. */
  const paths: Record<IconName, readonly string[]> = {
    send: ['M4 12 20 4l-5 16-3.5-6.5z', 'm11.5 13.5 8.5-9.5'],
    flow: [
      'M4.5 4h4A1.5 1.5 0 0 1 10 5.5v2A1.5 1.5 0 0 1 8.5 9h-4A1.5 1.5 0 0 1 3 7.5v-2A1.5 1.5 0 0 1 4.5 4z',
      'M15.5 15h4a1.5 1.5 0 0 1 1.5 1.5v2a1.5 1.5 0 0 1-1.5 1.5h-4a1.5 1.5 0 0 1-1.5-1.5v-2a1.5 1.5 0 0 1 1.5-1.5z',
      'M6.5 9v3.5a2.5 2.5 0 0 0 2.5 2.5h5',
    ],
    runs: ['M9 6h11M9 12h11M9 18h11', 'm3.5 6 1.3 1.3L7 5M3.5 12l1.3 1.3L7 11M3.5 18l1.3 1.3L7 17'],
    bot: [
      'M7.5 8h9a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3h-9a3 3 0 0 1-3-3v-5a3 3 0 0 1 3-3z',
      'M12 4v4M9 13h.01M15 13h.01M2.5 13v2M21.5 13v2',
    ],
    gear: [
      'M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
      'M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7',
    ],
    columns: [
      'M6 4.5h12a2.5 2.5 0 0 1 2.5 2.5v10a2.5 2.5 0 0 1-2.5 2.5H6A2.5 2.5 0 0 1 3.5 17V7A2.5 2.5 0 0 1 6 4.5z',
      'M12 4.5v15',
    ],
    search: ['M4.5 11a6.5 6.5 0 1 0 13 0a6.5 6.5 0 1 0-13 0', 'm20 20-4.4-4.4'],
    chevron: ['m6 9 6 6 6-6'],
  };

  let {
    name,
    small = false,
    class: extra = '',
  }: { name: IconName; small?: boolean; class?: string } = $props();
</script>

<svg class="ico {extra}" class:ico-sm={small} viewBox="0 0 24 24" aria-hidden="true">
  {#each paths[name] as d (d)}
    <path {d} />
  {/each}
</svg>
