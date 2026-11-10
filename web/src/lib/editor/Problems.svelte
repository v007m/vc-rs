<script lang="ts">
  import { Severity, type Diagnostic, type Fix } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import { byImportance } from './files.ts';

  let {
    diagnostics,
    live,
    ongo,
    onfix,
  }: {
    diagnostics: readonly Diagnostic[];
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
      <span class="pill" class:fail={errors > 0}>{count(errors, 'error')}</span>
      <span class="pill" class:block={warnings > 0}>{count(warnings, 'warning')}</span>
    </button>
    <span class="note"
      >{live
        ? 'Recomputed on every change, like vc check'
        : 'From the last check of the saved file'}</span
    >
  </div>
  {#if open}
    {#if sorted.length === 0}
      <p class="none">No problems in this file.</p>
    {:else}
      <ul>
        {#each sorted as d, i (i)}
          {@const error = d.severity === Severity.ERROR}
          <li class="prob" class:error>
            <button type="button" class="go" onclick={() => ongo(d)}>
              <span class="dot" class:fail={error} class:block={!error}></span>
              <span class="sr">{error ? 'Error' : 'Warning'}</span>
              <span class="mono code">{d.code}</span>
              <span class="msg">{d.message}</span>
              <span class="where">{where(d)}</span>
            </button>
            {#each d.help as help (help)}<p class="help">help: {help}</p>{/each}
            {#if d.fixes.length}
              <div class="fixes">
                {#each d.fixes as fix (fix.title)}
                  <button type="button" class="btn sm" onclick={() => onfix(fix)}
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
    border-top: 1px solid var(--line);
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
    font: 600 13px var(--font);
    color: var(--ink);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
  }
  .chev {
    width: 12px;
    color: var(--ink3);
  }
  .note,
  .none,
  .where {
    color: var(--ink3);
    font-size: 12px;
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
    border-bottom: 1px solid var(--line);
  }
  .go {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: 0;
    background: none;
    text-align: left;
    font: 13px var(--font);
    color: var(--ink);
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
  }
  .go:hover {
    background: var(--panel2);
  }
  .code {
    color: var(--ink2);
  }
  .msg {
    flex: 1 1 auto;
    min-width: 0;
  }
  .help {
    margin: 2px 0 0 30px;
    font-size: 12.5px;
    color: var(--ink2);
  }
  .fixes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 4px 0 2px 30px;
  }
</style>
