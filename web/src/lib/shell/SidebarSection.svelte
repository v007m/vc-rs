<script lang="ts">
  import { resolve } from '$app/paths';
  import Icon from './Icon.svelte';
  import type { ShellState } from './state.svelte.ts';
  import { viewOf, type Section, type SpaceKey } from './spaces.ts';
  import type { Snippet } from 'svelte';

  /**
   * One collapsible sidebar section (D515): a header row that toggles, and its view rows, or a
   * screen's own body (D537) with a count (`meta`) and header actions.
   */
  let {
    shell,
    space,
    section,
    pathname = '',
    meta,
    actions,
    children,
  }: {
    shell: ShellState;
    space: SpaceKey;
    section: Pick<Section, 'id' | 'label'> & Partial<Pick<Section, 'views'>>;
    pathname?: string;
    meta?: string;
    actions?: Snippet;
    children?: Snippet;
  } = $props();

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
      {#if meta}<span class="sb-sec-meta">{meta}</span>{/if}
    </button>
    {#if actions}<span class="sb-sec-acts">{@render actions()}</span>{/if}
    <Icon name="chevron" small class="sb-sec-chev" />
  </div>
  <div class="sb-sec-body" id="{uid}-body" hidden={!open}>
    {#if children}
      {@render children()}
    {:else}
      <ul class="sb-nav">
        {#each section.views ?? [] as view (view.route)}
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
    {/if}
  </div>
</section>
