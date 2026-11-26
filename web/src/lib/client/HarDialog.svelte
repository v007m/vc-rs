<script lang="ts">
  import DiffView from '#lib/editor/DiffView.svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { HarImport } from './har.svelte.ts';

  let {
    har,
    onclose,
    onwritten,
  }: { har: HarImport; onclose: () => void; onwritten: (titles: readonly string[]) => void } =
    $props();

  let open: string | undefined = $state();

  async function chosen(event: Event): Promise<void> {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (file) await har.choose(file);
  }

  async function write(): Promise<void> {
    await har.write();
    if (har.written) onwritten(har.written);
  }
</script>

<Dialog title="Import a HAR file" {onclose}>
  <div class="form">
    <label class="field">
      <span>HAR file</span>
      <input class="st-input" type="file" accept=".har,application/json" onchange={chosen} />
    </label>
    <label class="field">
      <span>Flow test</span>
      <input
        class="st-input vc-mono"
        placeholder="users.recorded#replaysSession"
        aria-describedby="har-flow-help"
        value={har.flow}
        oninput={(e) => har.setFlow(e.currentTarget.value)}
      />
    </label>
    <p class="help" id="har-flow-help">
      Optional: a test replaying the session in order, as <code>fileId#testId</code>. Static files,
      third-party hosts and preflights are dropped, and secrets become <code>${'{'}secret.…}</code> references.
    </p>
    {#if har.error}
      <p class="error" role="alert">{har.error}</p>
    {/if}
    {#if har.preview}
      {#each har.preview.titles as notice, i (i)}
        <p class="vc-mono notice">{notice}</p>
      {/each}
      <ul class="changes" aria-label="Changes">
        {#each har.preview.files as file (file.path)}
          <li>
            <button
              type="button"
              class="file"
              aria-expanded={open === file.path}
              onclick={() => (open = open === file.path ? undefined : file.path)}
            >
              <span class="vc-mono">{file.path}</span>
              <span class="st-badge {file.before ? 'vc-accent' : 'vc-pass'}"
                >{file.before ? 'changed' : 'new'}</span
              >
            </button>
            {#if open === file.path}
              <DiffView
                before={file.before}
                after={file.after}
                label="Changes to {file.path}"
                empty="No changes."
              />
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
      <button type="button" class="st-btn" disabled={!har.ready} onclick={() => har.showPreview()}
        >Preview</button
      >
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={!har.preview || har.busy}
        onclick={write}>Write files</button
      >
    </div>
  </div>
</Dialog>

<style>
  .form {
    display: grid;
    gap: 10px;
  }
  .field {
    display: grid;
    gap: 4px;
    font-weight: 500;
  }
  .help,
  .notice {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .error {
    margin: 0;
    color: var(--vc-fail);
  }
  .changes {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .file {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: var(--vc-radius-inner);
    background: var(--st-surface-wash);
    color: var(--st-text-primary);
    cursor: pointer;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
