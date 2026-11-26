<script lang="ts">
  import type { Snippet } from 'svelte';
  import { statusGlyph, statuses } from '#lib/results/status.ts';
  import { ago, runHref, span } from './links.ts';
  import { outcomeClass, outcomeLabel } from './outcome.ts';
  import type { RunLine } from './port.ts';

  /** All runs' table: a tick to compare, the run, what it ran where, its counts and its ⋯. */
  let {
    runs,
    picked,
    ontoggle,
    menu,
  }: {
    runs: readonly RunLine[];
    picked: readonly number[];
    ontoggle: (id: number, on: boolean) => void;
    menu: Snippet<[RunLine]>;
  } = $props();
</script>

<table class="vc-table runs">
  <caption class="vc-sr">Runs, newest first</caption>
  <thead>
    <tr>
      <th scope="col"><span class="vc-sr">Compare</span></th>
      <th scope="col">Run</th>
      <th scope="col">Selector</th>
      <th scope="col">Profile</th>
      <th scope="col">Branch</th>
      <th scope="col" class="r">Tests</th>
      <th scope="col" class="r">Duration</th>
      <th scope="col" class="r">Started</th>
      <th scope="col"><span class="vc-sr">Actions</span></th>
    </tr>
  </thead>
  <tbody>
    {#each runs as run (run.id)}
      <tr class:on={picked.includes(run.id)}>
        <td
          ><input
            type="checkbox"
            aria-label="Pick run #{run.id} to compare"
            checked={picked.includes(run.id)}
            onchange={(event) => ontoggle(run.id, event.currentTarget.checked)}
          /></td
        >
        <th scope="row">
          <a class="run" href={runHref(run)}
            ><span class="st-dot {outcomeClass[run.outcome]}" aria-hidden="true"></span><span
              class="vc-mono">#{run.id}</span
            >
            <span class="vc-sr">{outcomeLabel[run.outcome]}</span>
            {#if run.kind !== 'run'}<span class="st-badge">{run.kind}</span>{/if}</a
          >
        </th>
        <td class="vc-mono sel">{run.selector || 'all tests'}</td>
        <td>{run.profiles.join(', ') || '—'}</td>
        <td class="vc-mono"
          >{[run.branch, run.commit?.slice(0, 7)].filter(Boolean).join(' @ ') || '—'}</td
        >
        <td class="r counts">
          {#each statuses as status (status)}
            {#if run.counts[status]}<span class="vc-count" data-st={status} title={status}
                ><span aria-hidden="true">{statusGlyph[status]}</span>{run.counts[status]}<span
                  class="vc-sr"
                >
                  {status}</span
                ></span
              >{/if}
          {/each}
        </td>
        <td class="r">{span(run.durationMs)}</td>
        <td class="r st-secondary"
          ><time datetime={new Date(run.startedMs).toISOString()}>{ago(run.startedMs)}</time></td
        >
        <td class="act">{@render menu(run)}</td>
      </tr>
    {:else}
      <tr
        ><td colspan="9" class="st-secondary empty"
          >No runs match. Change a filter, or start a run.</td
        ></tr
      >
    {/each}
  </tbody>
</table>

<style>
  .runs {
    width: 100%;
  }
  .run {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: inherit;
    text-decoration: none;
    font-weight: 500;
  }
  .run:hover .vc-mono {
    text-decoration: underline;
  }
  .sel {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .counts {
    white-space: nowrap;
  }
  .counts .vc-count + .vc-count {
    margin-left: 8px;
  }
  .on {
    background: var(--st-surface-wash);
  }
  .act {
    width: 1px;
  }
  .empty {
    padding: 24px 8px;
    text-align: center;
  }
</style>
