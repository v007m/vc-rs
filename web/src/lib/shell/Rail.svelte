<script lang="ts">
  import { resolve } from '$app/paths';
  import Icon from './Icon.svelte';
  import { modAria, modHint } from './keys.ts';
  import type { ShellState } from './state.svelte.ts';
  import { spaceOf, spaces, type Space } from './spaces.ts';

  /**
   * The space switcher (D512): an icon rail on the canvas, or the row at the sidebar foot. While
   * the modifier is held, each space shows its shortcut (D600).
   */
  let {
    shell,
    pathname,
    where = 'rail',
  }: { shell: ShellState; pathname: string; where?: 'rail' | 'foot' } = $props();

  let current = $derived(spaceOf(pathname).key);
  const top = spaces.filter((space) => space.key !== 'settings');
  const end = spaces.filter((space) => space.key === 'settings');
  const keyHint = (space: Space) => modHint(space.shortcut);
</script>

{#snippet item(space: Space)}
  <li>
    <!-- eslint-disable svelte/no-navigation-without-resolve -- routeFor gives a resolved route, or one the shell remembered with its query -->
    <a
      class="sp-btn"
      href={shell.routeFor(space.key, pathname)}
      aria-label={space.label}
      aria-current={space.key === current ? 'page' : undefined}
      title="{space.label} ({keyHint(space)})"
      aria-keyshortcuts={modAria(space.shortcut)}
    >
      <Icon name={space.icon} />
      <span class="sp-label" aria-hidden="true">{space.label}</span>
      <span class="sp-hint" aria-hidden="true">{keyHint(space)}</span>
    </a>
    <!-- eslint-enable svelte/no-navigation-without-resolve -->
  </li>
{/snippet}

{#if where === 'rail'}
  <nav class="rail" aria-label="Spaces" data-region>
    <a class="rail-brand" href={resolve('/client')} aria-label="Vibranium" title="Vibranium">
      <svg class="mark" viewBox="0 0 28 28" aria-hidden="true">
        <path class="hex" d="M14 2 25 8.5v11L14 26 3 19.5v-11Z" />
        <path class="vee" d="M9 10.5 14 18l5-7.5" />
      </svg>
    </a>
    <ul class="rail-list">
      {#each top as space (space.key)}{@render item(space)}{/each}
    </ul>
    <ul class="rail-list rail-end">
      {#each end as space (space.key)}{@render item(space)}{/each}
    </ul>
  </nav>
{:else}
  <nav class="sb-spaces" aria-label="Spaces">
    <ul class="sb-spaces-list">
      {#each spaces as space (space.key)}{@render item(space)}{/each}
    </ul>
  </nav>
{/if}
