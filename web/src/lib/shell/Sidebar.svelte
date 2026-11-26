<script lang="ts">
  import Icon from './Icon.svelte';
  import Rail from './Rail.svelte';
  import type { ShellState } from './state.svelte.ts';
  import SidebarSection from './SidebarSection.svelte';
  import type { Space } from './spaces.ts';
  import type { Snippet } from 'svelte';

  /**
   * The current space's sidebar on L0: its title, its sections (or the screen's own `body`, D537),
   * and the foot switcher if chosen.
   */
  let {
    shell,
    space,
    pathname,
    body,
    ontoggle,
  }: {
    shell: ShellState;
    space: Space;
    pathname: string;
    body?: Snippet | undefined;
    ontoggle: () => void;
  } = $props();
</script>

<aside class="sidebar" id="sidebar" aria-label="{space.label} sidebar" data-region>
  <div class="sb-top">
    <h2 class="sb-title">{space.label}</h2>
    <button
      type="button"
      class="st-icon-btn st-icon-btn--sm sb-hide"
      aria-controls="sidebar"
      aria-label="Hide sidebar"
      title="Hide sidebar"
      onclick={ontoggle}
    >
      <Icon name="columns" />
    </button>
  </div>
  <div class="sb-body">
    <div class="sb-secs">
      {#if body}
        {@render body()}
      {:else}
        {#each space.sections as section (section.id)}
          <SidebarSection {shell} space={space.key} {section} {pathname} />
        {/each}
      {/if}
    </div>
  </div>
  {#if shell.prefs.switcher === 'foot'}
    <Rail {shell} {pathname} where="foot" />
  {/if}
</aside>
