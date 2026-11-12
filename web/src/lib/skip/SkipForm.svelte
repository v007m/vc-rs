<script lang="ts">
  import DiffView from '#lib/editor/DiffView.svelte';
  import type { SkipChange } from './skip.svelte.ts';

  /**
   * Writes or removes a test's `skip`: the date and reason for a skip, then a preview of the
   * file change, then the write. The engine refuses a change that adds an error (D107).
   */
  let {
    change,
    oncancel,
    onwritten,
  }: { change: SkipChange; oncancel: () => void; onwritten: () => void } = $props();

  const uid = $props.id();
  const verb = $derived(change.removing ? 'Remove skip' : 'Write skip');

  async function write(): Promise<void> {
    await change.write();
    if (change.written) onwritten();
  }
</script>

<div
  class="skipform"
  role="group"
  aria-label="{change.removing ? 'Unskip' : 'Skip'} {change.target}"
>
  {#if !change.removing}
    <label class="field">
      <span>Until</span>
      <input
        class="input"
        type="date"
        value={change.until}
        aria-describedby="{uid}-until"
        oninput={(e) => change.set('until', e.currentTarget.value)}
      />
    </label>
    <label class="field reason">
      <span>Reason</span>
      <input
        class="input"
        type="text"
        required
        value={change.reason}
        oninput={(e) => change.set('reason', e.currentTarget.value)}
      />
    </label>
    <p class="help" id="{uid}-until">
      Optional: the test runs again after this day, and <code>vc check</code> warns once it has passed.
    </p>
  {/if}
  {#if change.error}
    <p class="error" role="alert">{change.error}</p>
  {/if}
  {#if change.preview}
    {#each change.preview.files as file (file.path)}
      <DiffView
        before={file.before}
        after={file.after}
        label="Changes to {file.path}"
        empty="No changes."
      />
    {/each}
  {/if}
  <div class="actions">
    <button type="button" class="btn sm" onclick={oncancel}>Cancel</button>
    <button
      type="button"
      class="btn sm"
      disabled={!change.ready}
      onclick={() => void change.showPreview()}>Preview</button
    >
    <button
      type="button"
      class="btn sm primary"
      disabled={!change.preview || !change.ready}
      onclick={write}>{verb}</button
    >
  </div>
</div>

<style>
  .skipform {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 12px;
    align-items: end;
    flex-basis: 100%;
  }
  .field {
    display: grid;
    gap: 4px;
    font-size: 13px;
    font-weight: 500;
  }
  .reason {
    flex: 3 1 220px;
  }
  .help,
  .error {
    flex-basis: 100%;
    margin: 0;
    font-size: 12px;
    color: var(--ink2);
  }
  .error {
    color: var(--fail);
  }
  .skipform :global(.diff) {
    flex-basis: 100%;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
