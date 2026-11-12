<script lang="ts">
  import { tick } from 'svelte';
  import type { FlowEditor } from './flow.svelte.ts';
  import type { ApiInfo } from './port.ts';
  import SaveDraftDialog from './SaveDraftDialog.svelte';
  import StageLoop from './StageLoop.svelte';

  /**
   * The open flow test's stages as its file writes them (D251): the api each calls, the vars
   * it sets and saves, and how often it runs. Edits are saved through the guarded `EditFile`.
   */
  let {
    flow,
    apis,
    onsaved,
  }: { flow: FlowEditor; apis: readonly ApiInfo[]; onsaved: (what: string) => void } = $props();

  let adding = $state('');
  let saving = $state(false);
  const busy = $derived(flow.state !== 'idle' && flow.state !== 'conflict');

  let rows: HTMLTableSectionElement | undefined = $state();
  let moved = $state('');

  /** Moves stage `at`, says where it went and keeps focus on it, on the other button at an end. */
  async function move(at: number, by: -1 | 1): Promise<void> {
    flow.moveStage(at, by);
    const to = at + by;
    moved = `Stage ${at + 1} moved ${by < 0 ? 'up' : 'down'} to place ${to + 1}.`;
    await tick();
    const buttons = rows?.rows[to]?.querySelectorAll<HTMLButtonElement>('button[data-move]');
    const open = [...(buttons ?? [])].filter((button) => !button.disabled);
    (open.find((button) => button.dataset.move === String(by)) ?? open[0])?.focus();
  }

  function add(): void {
    if (!adding) return;
    flow.addStage(adding);
    adding = '';
  }
</script>

<section class="flow" aria-labelledby="flow-title">
  <div class="head">
    <h3 class="h3 grow" id="flow-title"><span class="mono">{flow.id}</span></h3>
    <button type="button" class="btn sm" disabled={!flow.dirty} onclick={() => flow.discard()}
      >Discard</button
    >
    <button
      type="button"
      class="btn sm primary"
      disabled={!flow.dirty || flow.problem !== undefined || busy}
      onclick={() => (saving = true)}>Save changes to flow</button
    >
  </div>
  {#if flow.state === 'conflict' && !saving}
    <div class="banner" role="alert">
      <span class="grow"
        >Not saved: <code>{flow.written?.path}</code> changed on disk since it was read.</span
      >
      <button type="button" class="btn sm" onclick={() => void flow.load()}>Reload from disk</button
      >
    </div>
  {/if}
  {#if flow.error && !saving}
    <p class="error" role="alert">{flow.error}</p>
  {/if}
  <table class="stages" aria-label="Stages">
    <thead>
      <tr>
        <th scope="col">#</th>
        <th scope="col">Request</th>
        <th scope="col">Uses</th>
        <th scope="col">Saves</th>
        <th scope="col">Runs</th>
        <th><span class="sr">Actions</span></th>
      </tr>
    </thead>
    <tbody bind:this={rows}>
      {#each flow.stages as stage, i (i)}
        <tr>
          <td>{i + 1}</td>
          <td>
            {#if stage.kind === 'call'}
              <select class="input mono" aria-label="Stage {i + 1} api" bind:value={stage.target}>
                {#if !apis.some((api) => api.id === stage.target)}
                  <option value={stage.target}>{stage.target}</option>
                {/if}
                {#each apis as api (api.id)}
                  <option value={api.id}>{api.id}</option>
                {/each}
              </select>
            {:else}
              <span class="mono">{stage.kind}: {stage.target}</span>
            {/if}
            {#if stage.name}<span class="hint mono">{stage.name}</span>{/if}
          </td>
          <td class="mono">{stage.uses.join(', ')}</td>
          <td class="mono">{stage.saves.join(', ')}</td>
          <td><StageLoop {stage} n={i + 1} /></td>
          <td class="actions">
            <button
              type="button"
              class="btn sm"
              data-move="-1"
              aria-label="Move up stage {i + 1}"
              disabled={i === 0}
              onclick={() => void move(i, -1)}>Move up</button
            >
            <button
              type="button"
              class="btn sm"
              data-move="1"
              aria-label="Move down stage {i + 1}"
              disabled={i === flow.stages.length - 1}
              onclick={() => void move(i, 1)}>Move down</button
            >
            <button
              type="button"
              class="btn sm"
              aria-label="Remove stage {i + 1}"
              onclick={() => flow.removeStage(i)}>Remove</button
            >
          </td>
        </tr>
      {:else}
        <tr><td colspan="6" class="hint">No stages. Add one below.</td></tr>
      {/each}
    </tbody>
  </table>
  <p class="sr" role="status">{moved}</p>
  <div class="add">
    <label class="sr" for="flow-add">Api to add</label>
    <select id="flow-add" class="input mono" bind:value={adding}>
      <option value="">Pick an api…</option>
      {#each apis as api (api.id)}
        <option value={api.id}>{api.id}</option>
      {/each}
    </select>
    <button type="button" class="btn sm" disabled={!adding} onclick={add}>Add stage</button>
  </div>
  {#if flow.problem}
    <p class="error">{flow.problem}</p>
  {:else if flow.dirty}
    <p class="hint">Unsaved: a run runs the flow as saved.</p>
  {/if}
</section>

{#if saving}
  <SaveDraftDialog
    request={flow}
    api={flow.id}
    onclose={() => (saving = false)}
    onsaved={(what) => {
      saving = false;
      onsaved(what);
    }}
  >
    {#snippet children(path: string)}
      The stages and how often each runs are written back to <code>{path}</code>, keeping its
      comments.
    {/snippet}
  </SaveDraftDialog>
{/if}

<style>
  .flow {
    display: grid;
    gap: 8px;
  }
  .head,
  .add,
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .banner {
    padding: 8px 12px;
    border: 1px solid var(--fail);
    border-radius: 6px;
  }
  .stages {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    font-weight: 500;
    text-align: left;
    color: var(--ink2);
  }
  td {
    padding: 4px 6px 4px 0;
    vertical-align: top;
  }
  .actions {
    white-space: nowrap;
  }
  .hint {
    display: block;
    margin: 0;
    color: var(--ink2);
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
</style>
