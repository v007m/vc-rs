<script lang="ts">
  import { resolve } from '$app/paths';
  import { doneCount } from '#lib/results/status.ts';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import Icon from './Icon.svelte';
  import type { ProjectStatus } from './project.svelte.ts';

  /** The one top row (D514), on L0: title · status on the left; env pill and search on the right. */
  let {
    title,
    session,
    project,
    sidebarShown,
    ontoggle,
  }: {
    title: string;
    session: RunSession;
    project: ProjectStatus;
    sidebarShown: boolean;
    ontoggle: () => void;
  } = $props();

  let view = $derived(session.view);
  let envTitle = $derived(
    project.profile
      ? `Environment: profile ${project.profile}. Change it in Settings.`
      : 'Environment',
  );
</script>

<header class="topbar">
  <button
    type="button"
    class="btn btn-ghost btn-icon btn-sm on-l0 sb-show"
    aria-controls="sidebar"
    aria-expanded={sidebarShown}
    aria-label="Show sidebar"
    title="Show sidebar"
    onclick={ontoggle}
  >
    <Icon name="columns" />
  </button>
  <h1 class="tb-title">{title}</h1>
  {#if session.running}
    <span class="tb-status hide-narrow" role="status">
      <span class="dot is-live" aria-hidden="true"></span>
      Running · {doneCount(view.counts)} of {view.planned || '…'}
    </span>
  {/if}
  <span class="grow"></span>
  <div class="topbar-end">
    <a class="btn env-pill on-l0" href={resolve('/settings')} title={envTitle}>
      <span class="env-cap">Env</span>
      <span class="tint-dot" data-p={project.profile} aria-hidden="true"></span>
      <span class="env-name">{project.profile ?? '—'}</span>
    </a>
    <button
      type="button"
      class="btn search-btn on-l0"
      aria-disabled="true"
      aria-keyshortcuts="Meta+K"
      title="Search or run (⌘K), coming soon"
    >
      <Icon name="search" />
      <span class="label">Search or run…</span>
      <span class="kbd">⌘K</span>
    </button>
  </div>
</header>
