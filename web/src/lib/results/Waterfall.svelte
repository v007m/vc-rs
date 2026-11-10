<script lang="ts">
  import { ms } from './status.ts';
  import { phases, reused, type TimingMs } from './waterfall.ts';

  /** One exchange's phases as bars on a shared time line, each with its time in text. */
  let { timing }: { timing: TimingMs } = $props();
  const laid = $derived(phases(timing));
</script>

<table class="waterfall" aria-label="Timing">
  <tbody>
    {#each laid as phase (phase.label)}
      <tr>
        <th scope="row">{phase.label}</th>
        <td class="track" aria-hidden="true">
          <span class="bar" style:margin-left="{phase.start}%" style:width="{phase.width}%"></span>
        </td>
        <td class="mono time">{ms(phase.ms)} ms</td>
      </tr>
    {/each}
  </tbody>
  <tfoot>
    <tr>
      <th scope="row">total</th>
      <td></td>
      <td class="mono time">{ms(timing.totalMs)} ms</td>
    </tr>
  </tfoot>
</table>
<p class="note">
  {reused(timing) ? 'Connection reused from an earlier request. ' : ''}firstByte is server time plus
  one network round trip.
</p>

<style>
  .waterfall {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    width: 7em;
    padding: 4px 8px 4px 0;
    font-weight: 500;
    color: var(--ink2);
    text-align: left;
  }
  .track {
    padding: 4px 0;
  }
  .bar {
    display: block;
    min-width: 2px;
    height: 10px;
    border-radius: 4px;
    background: var(--accent);
  }
  .time {
    width: 6em;
    padding-left: 12px;
    text-align: right;
    white-space: nowrap;
  }
  .note {
    margin: 8px 0 0;
    color: var(--ink2);
  }
  tfoot th,
  tfoot .time {
    border-top: 1px solid var(--line);
    color: var(--ink);
  }
</style>
