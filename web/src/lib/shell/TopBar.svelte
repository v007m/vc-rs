<script lang="ts">
  import { resolve } from '$app/paths';
  import { doneCount } from '#lib/results/status.ts';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import Icon from './Icon.svelte';
  import { modAria, modHint } from './keys.ts';
  import type { ProjectStatus } from './project.svelte.ts';
  import type { Snippet } from 'svelte';

  /**
   * The one top row (D514), chrome on the canvas: title · status on the left; the env pill and
   * the search field, an embossed capsule, on the right.
   */
  let {
    title,
    session,
    project,
    sidebarShown,
    left,
    ontoggle,
    onsearch,
  }: {
    title: string;
    session: RunSession;
    project: ProjectStatus;
    sidebarShown: boolean;
    /** The screen's own left side (the Client tabs, D537), in place of the title and status. */
    left?: Snippet | undefined;
    ontoggle: () => void;
    /** Opens the command palette out of the search field (D620). */
    onsearch: (field: HTMLElement) => void;
  } = $props();

  let view = $derived(session.view);
  let envTitle = $derived(
    project.profile
      ? `Environment: profile ${project.profile}. Change it in Settings.`
      : 'Environment',
  );
</script>

<header class="topbar" data-region>
  <button
    type="button"
    class="st-icon-btn st-icon-btn--sm sb-show"
    aria-controls="sidebar"
    aria-expanded={sidebarShown}
    aria-label="Show sidebar"
    title="Show sidebar"
    onclick={ontoggle}
  >
    <Icon name="columns" />
  </button>
  {#if left}
    <h1 class="vc-sr">{title}</h1>
    {@render left()}
  {:else}
    <h1 class="tb-title">{title}</h1>
  {/if}
  {#if session.running && !left}
    <span class="tb-status hide-narrow" role="status">
      <span class="st-dot vc-live" aria-hidden="true"></span>
      Running · {doneCount(view.counts)} of {view.planned || '…'}
    </span>
  {/if}
  {#if !left}<span class="vc-grow"></span>{/if}
  <div class="topbar-end">
    <a class="env-pill" href={resolve('/settings')} title={envTitle}>
      <span class="env-cap">Env</span>
      <span class="tint-dot" data-p={project.profile} aria-hidden="true"></span>
      <span class="env-name">{project.profile ?? '—'}</span>
    </a>
    <button
      type="button"
      class="search-btn"
      aria-haspopup="dialog"
      aria-keyshortcuts={modAria('K')}
      title="Search or run ({modHint('K')})"
      onclick={(event) => onsearch(event.currentTarget)}
    >
      <Icon name="search" />
      <span class="label">Search or run…</span>
      <span class="st-kbd">{modHint('K')}</span>
    </button>
  </div>
</header>
