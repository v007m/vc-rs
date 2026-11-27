<script lang="ts">
  import { copyText } from '#lib/agents/clipboard.ts';
  import Glyph from '#lib/ui/Glyph.svelte';
  import type { Toasts } from '#lib/ui/toasts.svelte.ts';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import { Staged } from '#lib/writes/staged.svelte.ts';
  import type { DemoPlan, OnboardingPort } from './port.ts';
  import StepNote from './StepNote.svelte';
  import { finish, loud, type Reload } from './steps.ts';

  /**
   * The demo project (D717): preview its files, write them with the preview's digest, and start
   * its service yourself; the browser shows the command to copy and never starts anything.
   */
  let { port, project, toasts }: { port: OnboardingPort; project: Reload; toasts: Toasts } =
    $props();

  const staged = new Staged();
  let plan: DemoPlan | undefined = $state.raw();
  let copied = $state('');

  const keep = (got: Awaited<ReturnType<OnboardingPort['demo']>>) => {
    if (got.available) plan = got.value;
    return got;
  };

  async function review(): Promise<void> {
    await staged.review(loud(toasts, async () => keep(await port.demo())));
  }

  async function write(): Promise<void> {
    const wrote = await staged.write(
      loud(toasts, async (digest: string) => keep(await port.demo(digest))),
    );
    if (!wrote || !plan) return;
    const { root, running, start } = plan;
    const action = running ? undefined : { label: 'Copy command', run: () => void copyText(start) };
    await finish(toasts, project, { text: `Wrote the demo project in ${root}`, action });
  }

  async function copy(text: string): Promise<void> {
    copied = await copyText(text);
  }
</script>

<div class="demo">
  <p class="lead">
    A small shop API with tests to run against it. Its service runs on your machine: start it in a
    terminal, since Vibranium never starts it for you.
  </p>
  {#if plan}
    {#if plan.running}
      <p>The demo service is answering at <code>{plan.url}</code>.</p>
    {:else}
      <p>The demo service is not running. Start it in a terminal:</p>
      <div class="code">
        <pre class="vc-mono">{plan.start}</pre>
        <button class="st-btn st-btn--sm" type="button" onclick={() => copy(plan?.start ?? '')}
          ><Glyph name={copied === 'Copied.' ? 'check' : 'copy'} />Copy</button
        >
      </div>
      <p class="lead" role="status">{copied}</p>
    {/if}
  {/if}
  <StepNote note={staged.note} />
  {#if staged.shown}
    <p class="lead">Files go in <span class="vc-mono">{plan?.root}</span>.</p>
    <ChangeList files={staged.shown.files} />
  {/if}
  <div class="actions">
    {#if staged.shown}
      <button type="button" class="st-btn" disabled={staged.busy} onclick={() => staged.reset()}
        >Back</button
      >
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={staged.busy}
        onclick={() => void write()}>{staged.busy ? 'Writing…' : 'Write files'}</button
      >
    {:else}
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={staged.busy}
        onclick={() => void review()}>{staged.busy ? 'Previewing…' : 'Preview the demo'}</button
      >
    {/if}
  </div>
</div>

<style>
  .demo {
    display: grid;
    gap: 12px;
  }
  p {
    margin: 0;
  }
  .lead {
    color: var(--st-text-secondary);
  }
  .code {
    display: flex;
    gap: 8px;
    align-items: start;
  }
  pre {
    flex: 1;
    margin: 0;
    padding: 12px;
    border-radius: 8px;
    background: var(--st-surface-wash);
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
