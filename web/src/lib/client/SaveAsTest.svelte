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

<Dialog title="Save as test" {onclose}>
  <form class="form" onsubmit={save}>
    <label class="field">
      <span>Test ID</span>
      <input
        class="input mono"
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
        <label class="check mono">
          <input type="checkbox" value={field} bind:group={fields} />
          {field}
        </label>
      {:else}
        <p class="help">The body has no text, number or boolean fields to expect.</p>
      {/each}
    </fieldset>
    {#if client.error}
      <p class="error" role="alert">{client.error}</p>
    {/if}
    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={saving || !testId.trim()}>Save</button>
    </div>
  </form>
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
    color: var(--ink2);
  }
  .fields {
    display: grid;
    gap: 4px;
    max-height: 240px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 9px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
