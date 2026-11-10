<script lang="ts">
  import { ms, statusClass } from '#lib/results/status.ts';
  import type { PlanRow, PlanState } from './plan.ts';

  let { plan }: { plan: readonly PlanRow[] } = $props();

  /** The pill modifier of each state; queued and running are not outcomes, so they have their own. */
  function pillOf(state: PlanState): string {
    if (state === 'queued') return '';
    return state === 'running' ? 'accent' : statusClass[state];
  }
</script>

<section class="plan">
  <h2 class="h2" id="plan-h">Tests</h2>
  <div class="scroll" role="region" aria-labelledby="plan-h">
    <table class="tbl">
      <thead>
        <tr><th scope="col">Test</th><th scope="col">State</th><th scope="col">ms</th></tr>
      </thead>
      <tbody>
        {#each plan as row (row.instance)}
          <tr>
            <td class="mono name">{row.label}</td>
            <td><span class="pill {pillOf(row.state)}">{row.state}</span></td>
            <td class="num">{ms(row.ms)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .plan {
    border-top: 1px solid var(--line);
    min-width: 0;
  }
  .plan .h2 {
    padding: 14px 20px 10px;
  }
  .scroll {
    max-height: 360px;
    overflow: auto;
    padding: 0 20px 12px;
  }
  .name {
    word-break: break-all;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
