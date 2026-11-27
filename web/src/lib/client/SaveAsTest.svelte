<script lang="ts">
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { ClientSession } from './session.svelte.ts';

  let {
    client,
    onclose,
    onsaved,
  }: { client: ClientSession; onclose: () => void; onsaved: (what: string) => void } = $props();

  let testId = $state('');
  let fields: string[] = $state([]);
  let saving = $state(false);
  const status = $derived(client.result?.response?.status);

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    saving = true;
    const added = await client.saveAsTest(testId.trim(), fields);
    saving = false;
    if (added) onsaved(added);
  }
</script>

<Dialog title="Save as test" {onclose} onsubmit={save}>
  <div class="form">
    <label class="field">
      <span>Test ID</span>
      <input
        class="st-input vc-mono"
        required
        placeholder="users.crud#createsUser"
        aria-describedby="save-id-help"
        bind:value={testId}
      />
    </label>
    <p class="help" id="save-id-help">
      <code>fileId#testId</code>, with a third-person verb (<code>rejectsBadEmail</code>). The test
      file is created when there is none.
    </p>
    <fieldset class="fields">
      <legend>Expect status {status} and these body fields at the values that came back</legend>
      {#each client.result?.fields ?? [] as field (field)}
        <label class="check vc-mono">
          <input type="checkbox" class="st-check" value={field} bind:group={fields} />
          {field}
        </label>
      {:else}
        <p class="help">The body has no text, number or boolean fields to expect.</p>
      {/each}
    </fieldset>
    {#if client.error}
      <p class="error" role="alert">{client.error}</p>
    {/if}
  </div>
  {#snippet actions()}
    <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
    <button type="submit" class="st-btn st-btn--primary" disabled={saving || !testId.trim()}
      >Save</button
    >
  {/snippet}
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
  .help {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .fields {
    display: grid;
    gap: 4px;
    max-height: 240px;
    overflow: auto;
    border-radius: var(--vc-radius-inner);
    background: var(--st-surface-wash);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .error {
    margin: 0;
    color: var(--vc-fail);
  }
</style>
