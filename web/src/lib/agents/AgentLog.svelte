<script lang="ts">
  import type { AgentAction } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import { ago } from './desk.svelte.ts';

  let { actions, now }: { actions: readonly AgentAction[]; now: number } = $props();

  let agent = $state('');
  let text = $state('');
  let open: number | undefined = $state();
  let agents = $derived([...new Set(actions.map((a) => a.agent))].sort());
  let shown = $derived(
    actions
      .map((action, index) => ({ action, index }))
      .filter(({ action }) => !agent || action.agent === agent)
      .filter(({ action }) => matchesText(action, text.trim().toLowerCase())),
  );

  function matchesText(action: AgentAction, needle: string): boolean {
    const hay = `${action.tool} ${action.arguments} ${action.result} ${action.profile ?? ''}`;
    return !needle || hay.toLowerCase().includes(needle);
  }
</script>

<section class="log" aria-labelledby="log-h">
  <h2 id="log-h" class="h2">Activity</h2>
  <div class="filters">
    <label>
      Agent
      <select class="input" bind:value={agent}>
        <option value="">All agents</option>
        {#each agents as name (name)}<option value={name}>{name}</option>{/each}
      </select>
    </label>
    <label class="grow">
      <span class="sr">Filter activity</span>
      <input class="input" type="search" placeholder="Filter activity" bind:value={text} />
    </label>
  </div>
  {#if shown.length}
    <table>
      <thead>
        <tr
          ><th scope="col">Tool</th><th scope="col">When</th><th scope="col">Agent</th><th
            scope="col">Profile</th
          ><th scope="col">Approved by</th></tr
        >
      </thead>
      <tbody>
        {#each shown as { action, index } (index)}
          <tr>
            <td>
              <button
                class="row mono"
                type="button"
                aria-expanded={open === index}
                aria-controls="act-{index}"
                onclick={() => (open = open === index ? undefined : index)}>{action.tool}</button
              >
            </td>
            <td>
              <time datetime={new Date(Number(action.atMs)).toISOString()}
                >{ago(action.atMs, now)}</time
              >
            </td>
            <td>{action.agent}</td>
            <td>{action.profile ?? '—'}</td>
            <td>{action.approvedBy ?? '—'}</td>
          </tr>
          {#if open === index}
            <tr id="act-{index}">
              <td colspan="5">
                <h3 class="sub">Arguments</h3>
                <pre class="mono">{action.arguments}</pre>
                <h3 class="sub">Result returned to {action.agent}</h3>
                <pre class="mono">{action.result}</pre>
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="empty">
      {actions.length ? 'No activity matches these filters.' : 'No agent has called a tool yet.'}
    </p>
  {/if}
</section>

<style>
  .log {
    display: grid;
    gap: 12px;
  }
  .filters {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  label {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--ink2);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }
  th {
    color: var(--ink2);
    font-weight: 500;
  }
  .row {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accentInk);
    font: inherit;
    cursor: pointer;
  }
  .sub {
    margin: 8px 0 4px;
    font-size: 13px;
    color: var(--ink2);
  }
  pre {
    margin: 0;
    padding: 10px;
    border-radius: 8px;
    background: var(--panel2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .empty {
    margin: 0;
    color: var(--ink2);
  }
</style>
