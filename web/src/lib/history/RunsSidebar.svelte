<script lang="ts">
  import { resolve } from '$app/paths';
  import { doneCount } from '#lib/results/status.ts';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import SidebarSection from '#lib/shell/SidebarSection.svelte';
  import { getShellState } from '#lib/shell/state.svelte.ts';
  import { spaceOf } from '#lib/shell/spaces.ts';
  import type { RunHistory } from './history.svelte.ts';
  import { ago, runHref } from './links.ts';
  import { outcomeClass, outcomeLabel } from './outcome.ts';

  /** The Runs sidebar (D631): Overview and All runs, then Recent, the live run first. */
  let {
    history,
    session,
    pathname,
  }: { history: RunHistory; session: RunSession; pathname: string } = $props();

  const shell = getShellState();
  const runs = spaceOf('/runs');
  const view = $derived(session.view);
  const live = $derived(view.phase !== 'idle');
</script>

{#each runs.sections as section (section.id)}
  <SidebarSection {shell} space="runs" {section} {pathname} />
{/each}
<SidebarSection {shell} space="runs" section={{ id: 'recent', label: 'Recent' }}>
  <ul class="sb-nav" aria-label="Recent runs">
    {#if live}
      {@const href = resolve('/runs/live')}
      <li>
        <a class="sb-row" {href} aria-current={pathname === href ? 'page' : undefined}>
          <span class="st-dot {session.running ? 'vc-live' : ''}" aria-hidden="true"></span>
          <span class="title">{session.running ? 'Live run' : 'Last live run'}</span>
          <span class="meta">
            {session.running ? `${doneCount(view.counts)} of ${view.planned || '…'}` : view.phase}
          </span>
        </a>
      </li>
    {/if}
    {#each history.recent as run (run.id)}
      {@const href = runHref(run)}
      <li>
        <a class="sb-row" {href} aria-current={pathname === href ? 'page' : undefined}>
          <span class="st-dot {outcomeClass[run.outcome]}" aria-hidden="true"></span>
          <span class="title"
            ><span class="vc-mono">#{run.id}</span>
            {run.kind === 'load' ? 'load' : run.selector || 'all tests'}<span class="vc-sr"
              >, {outcomeLabel[run.outcome]}</span
            ></span
          >
          <span class="meta">{ago(run.startedMs)}</span>
        </a>
      </li>
    {:else}
      {#if !live}<li class="sb-empty">No runs yet.</li>{/if}
    {/each}
  </ul>
</SidebarSection>

<style>
  .sb-row .st-dot {
    flex: none;
  }
  .sb-empty {
    padding: 4px 12px;
    color: var(--st-text-tertiary);
    font: var(--st-type-caption);
  }
</style>
