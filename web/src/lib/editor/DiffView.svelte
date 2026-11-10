<script lang="ts">
  import { foldDiff, lineDiff } from './text.ts';

  let { before, after }: { before: string; after: string } = $props();

  let lines = $derived(foldDiff(lineDiff(before, after)));
  const sign = { same: ' ', add: '+', del: '−' } as const;
  const said = { same: '', add: 'Added: ', del: 'Removed: ' } as const;
</script>

<section class="diff" aria-label="Changes since the last save">
  {#if !lines.some((l) => l.kind === 'add' || l.kind === 'del')}
    <p class="none">No changes since the last save.</p>
  {:else}
    <div class="scroll mono">
      {#each lines as line, i (i)}
        {#if line.kind === 'gap'}
          <div class="gap">{line.count} unchanged line{line.count === 1 ? '' : 's'}</div>
        {:else}
          <div class="ln {line.kind}">
            <span class="no" aria-hidden="true">{line.oldNo ?? ''}</span>
            <span class="no" aria-hidden="true">{line.newNo ?? ''}</span>
            <span class="sign" aria-hidden="true">{sign[line.kind]}</span>
            <span class="text"><span class="sr">{said[line.kind]}</span>{line.text}</span>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</section>

<style>
  .diff {
    min-height: 0;
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
  }
  .scroll {
    overflow: auto;
    border: 1px solid var(--line2);
    border-radius: 10px;
    background: var(--panel);
    padding: 6px 0;
  }
  .ln {
    display: grid;
    grid-template-columns: 3.5ch 3.5ch 2ch 1fr;
    white-space: pre;
    line-height: 20px;
  }
  .no {
    color: var(--ink3);
    text-align: right;
    padding-right: 6px;
  }
  .sign {
    text-align: center;
  }
  .add {
    background: var(--passSoft);
    color: var(--pass);
  }
  .del {
    background: var(--failSoft);
    color: var(--fail);
  }
  .gap {
    padding: 2px 12px;
    color: var(--ink3);
    background: var(--panel2);
    font-family: var(--font);
    font-size: 12px;
  }
  .none {
    color: var(--ink3);
  }
</style>
