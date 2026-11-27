<script lang="ts">
  import { onMount } from 'svelte';
  import { duration } from '#lib/insights/stats.ts';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import { lens } from '#lib/ui/segmented.ts';
  import { anyCalls, count, toolsByAgent, WINDOWS } from './usage.ts';
  import type { UsageView } from './usage.svelte.ts';
  import UsageChart from './UsageChart.svelte';

  /** What agents did through MCP over the last 7, 30 or 90 days, counted in calls (D709). */
  let { view }: { view: UsageView } = $props();

  onMount(() => void view.load());

  const usage = $derived(view.usage);
</script>

{#if view.missing}
  <EmptyState icon="bot" title="Agent usage is not available" caption={view.missing} />
{:else}
  <div class="usage" aria-busy={view.loading}>
    <div class="head">
      <p class="lead">
        The tool calls AI agents made through Vibranium’s MCP server, by day, agent and tool.
      </p>
      <div class="st-segmented" role="group" {@attach lens} aria-label="Window">
        {#each WINDOWS as days (days)}
          <button type="button" aria-pressed={view.days === days} onclick={() => view.show(days)}
            >{days} days</button
          >
        {/each}
      </div>
    </div>
    {#if view.error}
      <p class="error" role="alert">Could not load agent usage: {view.error}</p>
    {/if}
    {#if usage && !anyCalls(usage)}
      <EmptyState
        icon="bot"
        title="No agent calls in the last {view.days} days"
        caption="Connect an agent on the AI agents screen; its calls show here."
      />
    {:else if usage}
      <UsageChart {usage} />
      <table class="vc-table">
        <caption class="st-title3">By agent</caption>
        <thead>
          <tr>
            <th scope="col">Agent</th>
            <th scope="col" class="r">Calls</th>
            <th scope="col" class="r">Runs started</th>
            <th scope="col" class="r">Time spent</th>
          </tr>
        </thead>
        <tbody>
          {#each usage.agents as a (a.agent)}
            <tr>
              <th scope="row">{a.agent}</th>
              <td class="r">{count(a.total.calls)}</td>
              <td class="r">{count(a.total.runs)}</td>
              <td class="r">{duration(a.total.tookMs)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#each toolsByAgent(usage) as [agent, tools] (agent)}
        <table class="vc-table">
          <caption class="st-title3">Tools {agent} used</caption>
          <thead>
            <tr>
              <th scope="col">Tool</th>
              <th scope="col" class="r">Calls</th>
              <th scope="col" class="r">OK</th>
              <th scope="col" class="r">Failed</th>
              <th scope="col" class="r">Refused</th>
              <th scope="col" class="r">Approved</th>
            </tr>
          </thead>
          <tbody>
            {#each tools as t (t.tool)}
              <tr>
                <th scope="row" class="vc-mono">{t.tool}</th>
                <td class="r">{count(t.calls)}</td>
                <td class="r">{count(t.ok)}</td>
                <td class="r">{count(t.failed)}</td>
                <td class="r">{count(t.refused)}</td>
                <td class="r">{count(t.approved)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .usage {
    display: grid;
    gap: 24px;
    max-width: 960px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
  }
  .lead {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .error {
    margin: 0;
    color: var(--vc-fail);
  }
  caption {
    text-align: left;
    margin-bottom: 8px;
  }
  .r {
    text-align: right;
  }
</style>
