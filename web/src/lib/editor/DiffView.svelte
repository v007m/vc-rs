<script lang="ts">
  import { foldDiff, lineDiff } from './text.ts';

  let {
    before,
    after,
    label = 'Changes since the last save',
    empty = 'No changes since the last save.',
  }: { before: string; after: string; label?: string; empty?: string } = $props();

  let lines = $derived(foldDiff(lineDiff(before, after)));
  const sign = { same: ' ', add: '+', del: '−' } as const;
  const said = { same: '', add: 'Added: ', del: 'Removed: ' } as const;
</script>

<section class="diff" aria-label={label}>
  {#if !lines.some((l) => l.kind === 'add' || l.kind === 'del')}
    <p class="none">{empty}</p>
  {:else}
    <div class="scroll vc-mono">
      {#each lines as line, i (i)}
        {#if line.kind === 'gap'}
          <div class="gap">{line.count} unchanged line{line.count === 1 ? '' : 's'}</div>
        {:else}
          <div class="ln {line.kind}">
            <span class="no" aria-hidden="true">{line.oldNo ?? ''}</span>
            <span class="no" aria-hidden="true">{line.newNo ?? ''}</span>
            <span class="sign" aria-hidden="true">{sign[line.kind]}</span>
            <span class="text"><span class="vc-sr">{said[line.kind]}</span>{line.text}</span>
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
    border-radius: var(--vc-radius-inner);
    background: var(--st-surface-wash);
    padding: 6px 0;
  }
  .ln {
    display: grid;
    grid-template-columns: 3.5ch 3.5ch 2ch 1fr;
    white-space: pre;
    line-height: 20px;
  }
  .no {
    color: var(--st-text-tertiary);
    text-align: right;
    padding-right: 6px;
  }
  .sign {
    text-align: center;
  }
  .add {
    background: var(--vc-pass-soft);
    color: var(--vc-pass);
  }
  .del {
    background: var(--vc-fail-soft);
    color: var(--vc-fail);
  }
  .gap {
    padding: 2px 12px;
    color: var(--st-text-tertiary);
    background: var(--st-surface-wash);
    font-family: var(--st-font-sans);
    font-size: 12px;
  }
  .none {
    color: var(--st-text-tertiary);
  }
</style>
