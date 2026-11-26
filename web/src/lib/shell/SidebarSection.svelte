<script lang="ts">
  import { resolve } from '$app/paths';
  import Icon from './Icon.svelte';
  import type { ShellState } from './state.svelte.ts';
  import { viewOf, type Section, type SpaceKey } from './spaces.ts';

  /** One collapsible sidebar section (D515): a header row that toggles, and its view rows. */
  let {
    shell,
    space,
    section,
    pathname,
  }: { shell: ShellState; space: SpaceKey; section: Section; pathname: string } = $props();

  const uid = $props.id();
  let open = $derived(!shell.isClosed(space, section.id));
  let current = $derived(viewOf(pathname)?.route);
</script>

<section class="sb-sec" class:is-open={open} aria-labelledby="{uid}-h">
  <div class="sb-sec-head">
    <button
      type="button"
      class="sb-sec-btn"
      id="{uid}-h"
      aria-expanded={open}
      aria-controls="{uid}-body"
      onclick={() => shell.toggleSection(space, section.id)}
    >
      <span class="sb-sec-label">{section.label}</span>
    </button>
    <Icon name="chevron" small class="sb-sec-chev" />
  </div>
  <div class="sb-sec-body" id="{uid}-body" hidden={!open}>
    <ul class="sb-nav">
      {#each section.views as view (view.route)}
        <li>
          <a
            class="sb-row"
            href={resolve(view.route)}
            aria-current={view.route === current ? 'page' : undefined}
          >
            <span class="title">{view.label}</span>
          </a>
        </li>
      {/each}
    </ul>
  </div>
</section>
