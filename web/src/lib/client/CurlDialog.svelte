<script lang="ts">
  import Dialog from '#lib/ui/Dialog.svelte';

  /** The api as a curl command, resolved by the engine with its token as `$TOKEN`. */
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
    Resolved for the active profile. The bearer token is replaced by <code>$TOKEN</code>; print one
    with <code>vc auth token</code>.
  </p>
  <pre class="curl mono" aria-label="cURL command">{curl}</pre>
  <div class="actions">
    <p class="help grow" role="status">{copied}</p>
    <button type="button" class="btn" onclick={onclose}>Close</button>
    <button type="button" class="btn primary" onclick={() => void copy()}>Copy</button>
  </div>
</Dialog>

<style>
  .help {
    margin: 0 0 10px;
    color: var(--ink2);
  }
  .curl {
    margin: 0;
    padding: 8px 12px;
    overflow-x: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
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
