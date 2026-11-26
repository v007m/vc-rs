<script lang="ts">
  import Dialog from '#lib/ui/Dialog.svelte';
  import Glyph from '#lib/ui/Glyph.svelte';

  /** The request as a curl command, resolved by the engine, its auth left out. */
  let { curl, onclose }: { curl: string; onclose: () => void } = $props();

  let copied = $state('');

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(curl);
      copied = 'Copied.';
    } catch {
      copied = 'The browser did not allow copying; select the command instead.';
    }
  }
</script>

<Dialog title="Copy as cURL" {onclose}>
  <p class="help">
    Resolved for the active profile, with your unsaved edits and vars. Auth is left out because vc
    applies it; the first line names it. Use <code>vc call</code> to send it with auth.
  </p>
  <pre class="curl vc-mono" aria-label="cURL command">{curl}</pre>
  <div class="actions">
    <p class="help vc-grow" role="status">{copied}</p>
    <button type="button" class="st-btn" onclick={onclose}>Close</button>
    <button type="button" class="st-btn st-btn--primary" onclick={() => void copy()}
      ><Glyph name={copied === 'Copied.' ? 'check' : 'copy'} />Copy</button
    >
  </div>
</Dialog>

<style>
  .help {
    margin: 0 0 10px;
    color: var(--st-text-secondary);
  }
  .curl {
    margin: 0;
    padding: 8px 12px;
    overflow-x: auto;
    border-radius: var(--vc-radius-inner);
    background: var(--st-surface-wash);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }
  .actions .help {
    margin: 0;
  }
</style>
