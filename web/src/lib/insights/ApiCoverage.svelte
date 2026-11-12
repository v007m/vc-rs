<script lang="ts">
  import MethodTag from './MethodTag.svelte';
  import { testsLabel, type Code, type CoverageRow } from './coverage.ts';

  /** Each api and spec operation: its tests, and its documented statuses against those seen. */
  let {
    rows,
    notes,
    rangeLabel,
  }: { rows: readonly CoverageRow[]; notes: readonly string[]; rangeLabel: string } = $props();

  let gapsOnly = $state(false);
  const shown = $derived(gapsOnly ? rows.filter((r) => r.gap) : rows);
  const untested = $derived(rows.filter((r) => r.untested).length);
  const meaning: Record<Code['kind'], string> = {
    seen: 'documented and seen',
    missed: 'documented, never seen',
    undocumented: 'seen, not documented',
  };
  /** A note's `code` spans, odd parts between backticks. */
  const parts = (note: string) => note.split('`');
</script>

<section aria-labelledby="coverage" class="coverage">
  <h2 class="h2" id="coverage">API coverage</h2>
  <p class="muted">
    Every api in the api files and the operations of their openapi/ snapshots · statuses seen in the {rangeLabel}
  </p>
  {#each notes as note (note)}
    <p class="note">
      {#each parts(note) as part, i (i)}{#if i % 2 === 1}<code>{part}</code
          >{:else}{part}{/if}{/each}
    </p>
  {/each}
  {#if rows.length === 0}
    <p class="muted">No api is defined yet.</p>
  {:else}
    <div class="tools">
      <label class="toggle"
        ><input type="checkbox" bind:checked={gapsOnly} />Only gaps: untested or statuses never seen</label
      >
      <p class="legend" aria-hidden="true">
        <span><span class="code seen">201</span>{meaning.seen}</span>
        <span><span class="code missed">409</span>{meaning.missed}</span>
        <span><span class="code undocumented">503</span>{meaning.undocumented}</span>
      </p>
    </div>
    <div class="wrap">
      <table>
        <thead>
          <tr>
            <th scope="col">Api</th><th scope="col">Path · file</th><th scope="col">Tests</th><th
              scope="col">Statuses</th
            >
          </tr>
        </thead>
        <tbody>
          {#each shown as row (row.file + row.apiId)}
            <tr>
              <th scope="row" class="mono">
                <span class="id"><MethodTag method={row.method} /> {row.apiId}</span>
              </th>
              <td>
                <span class="mono path line" title={row.path}>{row.path}</span>
                <span class="muted small line"
                  >{row.specOnly ? `only in ${row.file}` : row.file}</span
                >
              </td>
              <td>
                <span class="line">{testsLabel(row)}</span>
                {#if row.untested}<span class="none line">untested</span>{/if}
              </td>
              <td>
                <span class="codes">
                  {#each row.codes as code (code.text)}
                    <span class="code {code.kind}" title={meaning[code.kind]}
                      >{code.text}<span class="sr">: {meaning[code.kind]};</span></span
                    >
                  {/each}
                </span>
              </td>
            </tr>
          {:else}
            <tr
              ><td colspan="4" class="muted"
                >No gaps: every api was called and every status seen.</td
              ></tr
            >
          {/each}
        </tbody>
      </table>
    </div>
    <p class="muted small">
      {rows.length - untested} of {rows.length} apis called in the range · {untested} untested. Untested
      means no call of it is stored in the range; Tests counts the tests that call it, skipped ones included.
    </p>
  {/if}
</section>

<style>
  .coverage p {
    margin: 4px 0 0;
  }
  .note {
    color: var(--block);
    font-size: 13px;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    align-items: center;
    margin-top: 8px;
    font-size: 13px;
  }
  .toggle {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    font-size: 12px;
    color: var(--ink2);
  }
  .legend > span {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .wrap {
    max-height: 430px;
    margin-top: 8px;
    overflow: auto;
  }
  table {
    width: 100%;
    min-width: 600px;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--panel);
    color: var(--ink2);
    font-weight: 500;
  }
  th,
  td {
    padding: 7px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: middle;
  }
  tbody th {
    font-weight: 400;
    white-space: nowrap;
  }
  .id {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .line {
    display: block;
  }
  .path {
    font-size: 11.5px;
    color: var(--ink2);
  }
  .small {
    font-size: 11.5px;
  }
  .none {
    color: var(--block);
    font-weight: 600;
  }
  .codes {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .code {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 6px;
    border-radius: 5px;
    font: 500 11px var(--mono);
  }
  .code.seen {
    background: var(--passSoft);
    color: var(--pass);
  }
  .code.missed {
    box-shadow: inset 0 0 0 1px var(--line2);
    color: var(--ink2);
    text-decoration: line-through;
    text-decoration-color: var(--ink3);
  }
  .code.undocumented {
    background: var(--blockSoft);
    color: var(--block);
  }
  .muted {
    color: var(--ink2);
  }
</style>
