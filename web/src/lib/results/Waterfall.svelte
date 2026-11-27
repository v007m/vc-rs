<script lang="ts">
  import { ms } from './status.ts';
  import { reused, waterfall, type Timing } from './timing.ts';

  let { timing }: { timing: Timing } = $props();
  const bars = $derived(waterfall(timing));
</script>

<figure class="wf">
  <table class="vc-mono" aria-label="Timing waterfall">
    <tbody>
      {#each bars as bar (bar.name)}
        <tr>
          <th scope="row">{bar.name}</th>
          <td class="track" aria-hidden="true">
            <span class="bar {bar.name}" style:left="{bar.start}%" style:width="{bar.width}%"
            ></span>
          </td>
          <td class="num">{ms(bar.ms)} ms</td>
        </tr>
      {/each}
      <tr class="total vc-rule-top">
        <th scope="row">total</th>
        <td aria-hidden="true"></td>
        <td class="num">{ms(timing.totalMs)} ms</td>
      </tr>
    </tbody>
  </table>
  <figcaption class="note">
    {reused(timing) ? 'Connection reused from an earlier request. ' : ''}firstByte is server time
    plus one network round trip. A timing check, such as timing.total lt 500, fails the call when it
    is slow.
  </figcaption>
</figure>

<style>
  .wf {
    margin: 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  th {
    width: 96px;
    padding: 5px 10px 5px 0;
    font-weight: 500;
    text-align: left;
    color: var(--st-text-secondary);
  }
  .track {
    position: relative;
    height: 12px;
    background: var(--st-surface-wash);
    border-radius: 4px;
    overflow: hidden;
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 2px;
    border-radius: 3px;
    background: var(--st-accent);
  }
  .bar.dns {
    background: var(--vc-wf-dns);
  }
  .bar.connect {
    background: var(--vc-wf-connect);
  }
  .bar.tls {
    background: var(--vc-wf-tls);
  }
  .bar.firstByte {
    background: var(--vc-wf-wait);
  }
  .bar.download {
    background: var(--vc-wf-down);
  }
  .num {
    width: 72px;
    padding-left: 10px;
    text-align: right;
  }
  .total th,
  .total td {
    padding-top: 9px;
    font-weight: 600;
    color: var(--st-text-primary);
  }
  .note {
    margin-top: 8px;
    font-size: 12.5px;
    color: var(--st-text-secondary);
  }
</style>
