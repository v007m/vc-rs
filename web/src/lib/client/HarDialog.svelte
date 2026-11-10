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
      <input class="input" type="file" accept=".har,application/json" onchange={chosen} />
    </label>
    <label class="field">
      <span>Flow test</span>
      <input
        class="input mono"
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
        <p class="mono notice">{notice}</p>
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
              <span class="mono">{file.path}</span>
              <span class="pill {file.before ? 'accent' : 'pass'}"
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
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="button" class="btn" disabled={!har.ready} onclick={() => har.showPreview()}
        >Preview</button
      >
      <button type="button" class="btn primary" disabled={!har.preview || har.busy} onclick={write}
        >Write files</button
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
    color: var(--ink2);
  }
  .error {
    margin: 0;
    color: var(--fail);
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
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
    color: var(--ink);
    cursor: pointer;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
