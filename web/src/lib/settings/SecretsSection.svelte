<script lang="ts">
  import type { SecretSchema } from './secrets.svelte.ts';

  /** The secret names `.env.schema` declares, edited in place and saved together (D247). */
  let { schema }: { schema: SecretSchema } = $props();

  let name = $state('');
  let problem: string | undefined = $state();
  let busy = $derived(schema.state === 'loading' || schema.state === 'saving');

  function add(event: SubmitEvent): void {
    event.preventDefault();
    problem = schema.add(name.trim());
    if (problem === undefined) name = '';
  }
</script>

<section class="secrets" aria-labelledby="secrets-h">
  <h2 id="secrets-h" class="h2">Secrets</h2>
  <p class="intro">
    The names <code>.env.schema</code> declares. A <code>{'${secret.NAME}'}</code> must be declared
    here; its value comes from <code>.env</code> or your secrets provider, never from this page.
  </p>
  {#if schema.error}
    <p class="error" role="alert">{schema.error}</p>
  {/if}
  {#if schema.state === 'conflict'}
    <div class="banner" role="alert">
      <span class="grow"
        >Not saved: <code>.env.schema</code> changed on disk since you opened it.</span
      >
      <button type="button" class="btn sm" onclick={() => void schema.load()}
        >Reload from disk</button
      >
    </div>
  {/if}
  <table aria-busy={busy}>
    <caption class="sr">Declared secrets</caption>
    <thead>
      <tr>
        <th scope="col">Name</th><th scope="col">Annotations</th><th scope="col">Note</th><th
          scope="col"><span class="sr">Actions</span></th
        >
      </tr>
    </thead>
    <tbody>
      {#each schema.entries as entry (entry.name)}
        <tr>
          <th scope="row" class="mono">{entry.name}</th>
          <td class="mono">{entry.annotations.join(' ')}</td>
          <td>{entry.notes.join(' ')}</td>
          <td class="act">
            <button
              type="button"
              class="btn sm"
              aria-label="Remove {entry.name}"
              disabled={busy}
              onclick={() => schema.remove(entry.name)}>Remove</button
            >
          </td>
        </tr>
      {:else}
        <tr>
          <td colspan="4" class="empty"
            >{schema.state === 'loading' ? 'Loading…' : 'No secrets are declared yet.'}</td
          >
        </tr>
      {/each}
    </tbody>
  </table>
  <form class="add" onsubmit={add}>
    <label for="secret-name">New secret name</label>
    <input
      id="secret-name"
      class="input mono"
      autocomplete="off"
      spellcheck="false"
      placeholder="SHOP_TOKEN"
      aria-invalid={problem !== undefined}
      aria-describedby={problem ? 'secret-name-problem' : undefined}
      bind:value={name}
      oninput={() => (problem = undefined)}
    />
    <button type="submit" class="btn sm" disabled={busy || !name.trim()}>Add</button>
    {#if problem}<p id="secret-name-problem" class="error">{problem}</p>{/if}
  </form>
  <div class="actions">
    <button
      type="button"
      class="btn primary"
      disabled={busy || !schema.dirty}
      onclick={() => void schema.save()}>{schema.state === 'saving' ? 'Saving…' : 'Save'}</button
    >
    <button
      type="button"
      class="btn"
      disabled={busy || !schema.dirty}
      onclick={() => schema.discard()}>Discard changes</button
    >
    <p class="status" role="status">
      {#if schema.dirty}
        Unsaved changes.
      {:else if schema.saved?.checkError}
        Saved, but the project could not be checked: {schema.saved.checkError}
      {:else if schema.saved}
        Saved. {schema.saved.problems === 0
          ? 'The project checks clean.'
          : `The project has ${schema.saved.problems} problem${schema.saved.problems === 1 ? '' : 's'}; see them in the editor.`}
      {/if}
    </p>
  </div>
</section>

<style>
  .secrets {
    display: grid;
    gap: 12px;
  }
  .intro,
  .empty,
  .status {
    margin: 0;
    color: var(--ink2);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    color: var(--ink2);
    font-weight: 500;
  }
  tbody th {
    font-weight: 400;
  }
  .act {
    width: 1%;
    white-space: nowrap;
  }
  .add,
  .actions,
  .banner {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .add .error {
    flex-basis: 100%;
  }
  .banner {
    padding: 8px 12px;
    border: 1px solid var(--fail);
    border-radius: 6px;
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
</style>
