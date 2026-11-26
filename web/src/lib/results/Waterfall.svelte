<script lang="ts">
  import { ms } from './status.ts';
  import { reused, waterfall, type Timing } from './timing.ts';

  let { timing }: { timing: Timing } = $props();
  const bars = $derived(waterfall(timing));
</script>

<figure class="wf">
  <table class="mono" aria-label="Timing waterfall">
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
      <tr class="total">
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
    color: var(--ink2);
  }
  .track {
    position: relative;
    height: 12px;
    background: var(--panel2);
    border-radius: 4px;
    overflow: hidden;
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 2px;
    border-radius: 3px;
    background: var(--accent);
  }
  .bar.dns {
    background: var(--wf-dns);
  }
  .bar.connect {
    background: var(--wf-connect);
  }
  .bar.tls {
    background: var(--wf-tls);
  }
  .bar.firstByte {
    background: var(--wf-wait);
  }
  .bar.download {
    background: var(--wf-down);
  }
  .num {
    width: 72px;
    padding-left: 10px;
    text-align: right;
  }
  .total th,
  .total td {
    padding-top: 9px;
    border-top: 1px solid var(--line);
    font-weight: 600;
    color: var(--ink);
  }
  .note {
    margin-top: 8px;
    font-size: 12.5px;
    color: var(--ink2);
  }
</style>
