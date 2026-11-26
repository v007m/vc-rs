<script lang="ts">
  import { Severity, type Diagnostic, type Fix } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import { byImportance } from './files.ts';

  let {
    diagnostics,
    failure,
    live,
    ongo,
    onfix,
  }: {
    diagnostics: readonly Diagnostic[];
    /** Why the last check could not run; shown in place of its diagnostics. */
    failure?: string | undefined;
    /** Whether the list follows the unsaved text, or only the last check of the file on disk. */
    live: boolean;
    ongo: (diagnostic: Diagnostic) => void;
    onfix: (fix: Fix) => void;
  } = $props();

  let open = $state(true);
  let sorted = $derived(byImportance(diagnostics));
  let errors = $derived(diagnostics.filter((d) => d.severity === Severity.ERROR).length);
  let warnings = $derived(diagnostics.length - errors);

  function count(n: number, word: string): string {
    return `${n} ${word}${n === 1 ? '' : 's'}`;
  }

  function where(d: Diagnostic): string {
    const line = d.primary?.span?.startLine;
    return line ? `line ${line}` : '';
  }
</script>

<section class="probs" aria-label="Problems">
  <div class="head">
    <button type="button" class="title" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="chev" aria-hidden="true">{open ? '▾' : '▸'}</span>Problems
      <span class="st-badge" class:vc-fail={errors > 0}>{count(errors, 'error')}</span>
      <span class="st-badge" class:vc-blocked={warnings > 0}>{count(warnings, 'warning')}</span>
    </button>
    <span class="note"
      >{live
        ? 'Recomputed on every change, like vc check'
        : 'From the last check of the saved file'}</span
    >
  </div>
  {#if open}
    {#if failure}
      <p class="none failed">The file was saved, but checking it failed: {failure}</p>
    {:else if sorted.length === 0}
      <p class="none">No problems in this file.</p>
    {:else}
      <ul>
        {#each sorted as d, i (i)}
          {@const error = d.severity === Severity.ERROR}
          <li class="prob" class:error>
            <button type="button" class="go" onclick={() => ongo(d)}>
              <span class="st-dot" class:vc-fail={error} class:vc-blocked={!error}></span>
              <span class="vc-sr">{error ? 'Error' : 'Warning'}</span>
              <span class="vc-mono code">{d.code}</span>
              <span class="msg">{d.message}</span>
              <span class="where">{where(d)}</span>
            </button>
            {#each d.help as help (help)}<p class="help">help: {help}</p>{/each}
            {#if d.fixes.length}
              <div class="fixes">
                {#each d.fixes as fix (fix.title)}
                  <button type="button" class="st-btn st-btn--sm" onclick={() => onfix(fix)}
                    >{fix.title}</button
                  >
                {/each}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .probs {
    border-top: 1px solid var(--st-chrome-separator);
    padding: 8px 0 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: none;
    font: 600 13px var(--st-font-sans);
    color: var(--st-text-primary);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
  }
  .chev {
    width: 12px;
    color: var(--st-text-tertiary);
  }
  .note,
  .none,
  .where {
    color: var(--st-text-tertiary);
    font-size: 12px;
  }
  .none.failed {
    color: var(--vc-fail);
  }
  ul {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    max-height: 220px;
    overflow: auto;
  }
  .prob {
    padding: 4px 0;
    border-bottom: 1px solid var(--st-chrome-separator);
  }
  .go {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: 0;
    background: none;
    text-align: left;
    font: 13px var(--st-font-sans);
    color: var(--st-text-primary);
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
  }
  .go:hover {
    background: var(--st-surface-wash);
  }
  .code {
    color: var(--st-text-secondary);
  }
  .msg {
    flex: 1 1 auto;
    min-width: 0;
  }
  .help {
    margin: 2px 0 0 30px;
    font-size: 12.5px;
    color: var(--st-text-secondary);
  }
  .fixes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 4px 0 2px 30px;
  }
</style>
