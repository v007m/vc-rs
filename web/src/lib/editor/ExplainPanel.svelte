<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import type { Maybe } from '#lib/api/maybe.ts';
  import Markdown from '#lib/ui/Markdown.svelte';
  import type { Explained, ExplainPort } from './seams.ts';

  /**
   * `vc explain` beside the code (D651): a diagnostic code's page with its Wrong and Right
   * examples, or a test's or api's plain text, masked by the engine.
   */
  let { port, target, onclose }: { port: ExplainPort; target: string; onclose: () => void } =
    $props();

  const uid = $props.id();
  let got: Maybe<Explained> | undefined = $state.raw();
  let error: string | undefined = $state();

  $effect(() => {
    let live = true;
    got = undefined;
    error = undefined;
    port.explain(target).then(
      (value) => live && (got = value),
      (reason: unknown) => live && (error = ConnectError.from(reason).rawMessage),
    );
    return () => (live = false);
  });
</script>

<aside class="explain" aria-labelledby="{uid}-t">
  <div class="head">
    <h2 class="st-title3" id="{uid}-t">Explain <span class="vc-mono">{target}</span></h2>
    <button type="button" class="st-icon-btn st-icon-btn--sm" aria-label="Close" onclick={onclose}
      ><span aria-hidden="true">✕</span></button
    >
  </div>
  {#if error}
    <p class="st-secondary" role="status">{error}</p>
  {:else if !got}
    <p class="st-secondary" role="status">Explaining…</p>
  {:else if !got.available}
    <p class="st-secondary" role="status">{got.reason}</p>
  {:else}
    {#if got.value.markdown}
      <Markdown text={got.value.text} />
    {:else}
      <pre class="vc-mono text">{got.value.text}</pre>
    {/if}
    {#each got.value.notices as notice (notice)}<p class="st-secondary note">{notice}</p>{/each}
  {/if}
</aside>

<style>
  .explain {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    overflow: auto;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h2 {
    flex: 1 1 auto;
    margin: 0;
  }
  .text {
    margin: 0;
    font-size: 12.5px;
    white-space: pre-wrap;
  }
  .note {
    margin: 0;
    font-size: 12px;
  }
</style>
