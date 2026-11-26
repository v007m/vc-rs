<script lang="ts">
  import Icon from './Icon.svelte';
  import Rail from './Rail.svelte';
  import type { ShellState } from './state.svelte.ts';
  import SidebarSection from './SidebarSection.svelte';
  import type { Space } from './spaces.ts';

  /** The current space's sidebar on L0: its title, its sections, and the foot switcher if chosen. */
  let {
    shell,
    space,
    pathname,
    ontoggle,
  }: { shell: ShellState; space: Space; pathname: string; ontoggle: () => void } = $props();
</script>

<aside class="sidebar" id="sidebar" aria-label="{space.label} sidebar">
  <div class="sb-top">
    <h2 class="sb-title">{space.label}</h2>
    <button
      type="button"
      class="btn btn-ghost btn-icon btn-sm on-l0 sb-hide"
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
      {#each space.sections as section (section.id)}
        <SidebarSection {shell} space={space.key} {section} {pathname} />
      {/each}
    </div>
  </div>
  {#if shell.prefs.switcher === 'foot'}
    <Rail {shell} {pathname} where="foot" />
  {/if}
</aside>
