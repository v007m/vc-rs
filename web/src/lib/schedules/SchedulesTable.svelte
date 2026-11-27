<script lang="ts">
  import { resolve } from '$app/paths';
  import { ago, runHref } from '#lib/history/links.ts';
  import { outcomeClass, outcomeLabel } from '#lib/history/outcome.ts';
  import Menu from '#lib/ui/Menu.svelte';
  import { runnerLabel, type ScheduleLine } from './port.ts';
  import type { ScheduleList } from './schedules.svelte.ts';
  import { cronWords, untilWords } from './words.ts';

  /**
   * The schedules as flat rows (ui.md, Schedules): the on/off switch, the schedule (a link to
   * its runs), the cron in words, what runs where, when it next fires, its last run and its ⋯.
   */
  let {
    list,
    onpick,
  }: {
    list: ScheduleList;
    /** A row's ⋯ action: `edit`, `export` or `remove`. */
    onpick: (line: ScheduleLine, action: string) => void;
  } = $props();

  const actions = [
    { id: 'edit', label: 'Edit…' },
    { id: 'export', label: 'Export to GitHub Actions…' },
    { id: 'remove', label: 'Remove…' },
  ];
  const runsHref = (id: string) => `${resolve('/runs')}?schedule=${encodeURIComponent(id)}`;
  const at = (ms: number) => new Date(ms);
</script>

<table class="vc-table schedules">
  <caption class="vc-sr">Schedules</caption>
  <thead>
    <tr>
      <th scope="col"><span class="vc-sr">On</span></th>
      <th scope="col">Schedule</th>
      <th scope="col">When</th>
      <th scope="col">Selector</th>
      <th scope="col">Profiles</th>
      <th scope="col">Runs on</th>
      <th scope="col">Next run</th>
      <th scope="col">Last run</th>
      <th scope="col"><span class="vc-sr">Actions</span></th>
    </tr>
  </thead>
  <tbody>
    {#each list.lines as line (line.id)}
      {@const words = cronWords(line.cron)}
      <tr class:off={!line.enabled}>
        <td
          ><input
            type="checkbox"
            role="switch"
            class="st-switch"
            aria-label="Schedule {line.id}"
            checked={line.enabled}
            disabled={list.toggling !== undefined}
            onchange={(event) => void list.toggle(line, event.currentTarget.checked)}
          /></td
        >
        <th scope="row">
          <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- runsHref is resolve('/runs') plus a query -->
          <a class="name" href={runsHref(line.id)} title="Show its runs">{line.name ?? line.id}</a>
          {#if line.name}<span class="vc-mono id">{line.id}</span>{/if}
        </th>
        <td>
          {#if words}{words}<span class="vc-mono cron">{line.cron}</span>{:else}<span
              class="vc-mono">{line.cron}</span
            >{/if}
        </td>
        <td class="vc-mono sel">{line.selector.join(' ') || 'all tests'}</td>
        <td>{line.profiles.join(', ') || 'default'}</td>
        <td>{runnerLabel[line.runner] ?? line.runner}</td>
        <td class="st-secondary">
          {#if line.nextMs !== undefined}
            <time datetime={at(line.nextMs).toISOString()} title={at(line.nextMs).toLocaleString()}
              >{untilWords(line.nextMs)}</time
            >
          {:else}—{/if}
        </td>
        <td>
          {#if line.last}
            {@const last = line.last}
            <a class="run" href={runHref(last)}
              ><span class="st-dot {outcomeClass[last.outcome]}" aria-hidden="true"></span><span
                class="vc-mono">#{last.id}</span
              >
              {outcomeLabel[last.outcome]}</a
            >
            <time
              class="st-secondary"
              datetime={at(last.startedMs).toISOString()}
              title={at(last.startedMs).toLocaleString()}>{ago(last.startedMs)}</time
            >
          {:else}<span class="st-secondary">Not run yet</span>{/if}
        </td>
        <td class="act">
          <Menu
            label="{line.id} actions"
            button="st-icon-btn"
            items={actions}
            onpick={(id) => onpick(line, id)}><span aria-hidden="true">⋯</span></Menu
          >
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  .schedules {
    width: 100%;
  }
  .name,
  .run {
    color: inherit;
    text-decoration: none;
    font-weight: 500;
  }
  .name:hover,
  .run:hover .vc-mono {
    text-decoration: underline;
  }
  .id,
  .cron {
    display: block;
    font-size: 12px;
    font-weight: 400;
    color: var(--st-text-secondary);
  }
  .run {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .run + time {
    display: block;
    font-size: 12px;
  }
  .sel {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .off .name {
    color: var(--st-text-secondary);
  }
  .act {
    width: 1px;
  }
</style>
