<script lang="ts">
  import type { SecretCheck } from './secretCheck.svelte.ts';
  import type { SecretSchema } from './secrets.svelte.ts';

  /** The secret names `.env.schema` declares, edited in place and saved together (D247), and
   *  under each profile whether they resolve, never their values. */
  let {
    schema,
    check,
    profiles = [],
  }: { schema: SecretSchema; check?: SecretCheck; profiles?: readonly string[] } = $props();
  let columns = $derived(check ? profiles : []);

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
  <h2 id="secrets-h" class="st-title3">Secrets</h2>
  <p class="intro">
    The names <code>.env.schema</code> declares. A <code>{'${secret.NAME}'}</code> must be declared
    here; its value comes from <code>.env</code> or your secrets provider, never from this page.
  </p>
  {#if schema.error}
    <p class="error" role="alert">{schema.error}</p>
  {/if}
  {#if schema.state === 'conflict'}
    <div class="vc-banner" role="alert">
      <span class="vc-grow"
        >Not saved: <code>.env.schema</code> changed on disk since you opened it.</span
      >
      <button type="button" class="st-btn st-btn--sm" onclick={() => void schema.load()}
        >Reload from disk</button
      >
    </div>
  {/if}
  <table aria-busy={busy}>
    <caption class="vc-sr">Declared secrets</caption>
    <thead>
      <tr>
        <th scope="col">Name</th><th scope="col">Annotations</th><th scope="col">Note</th>
        {#each columns as profile (profile)}<th scope="col" class="vc-mono">{profile}</th>{/each}
        <th scope="col"><span class="vc-sr">Actions</span></th>
      </tr>
    </thead>
    <tbody>
      {#each schema.entries as entry (entry.name)}
        <tr>
          <th scope="row" class="vc-mono">{entry.name}</th>
          <td class="vc-mono">{entry.annotations.join(' ')}</td>
          <td>{entry.notes.join(' ')}</td>
          {#each columns as profile (profile)}
            {@const state = check?.state(entry.name, profile) ?? 'not checked'}
            <td
              class="state {state.replace(' ', '-')}"
              aria-label="{entry.name} on {profile}: {state}">{state}</td
            >
          {/each}
          <td class="act">
            <button
              type="button"
              class="st-btn st-btn--sm"
              aria-label="Remove {entry.name}"
              disabled={busy}
              onclick={() => schema.remove(entry.name)}>Remove</button
            >
          </td>
        </tr>
      {:else}
        <tr>
          <td colspan={4 + columns.length} class="empty"
            >{schema.state === 'loading' ? 'Loading…' : 'No secrets are declared yet.'}</td
          >
        </tr>
      {/each}
    </tbody>
  </table>
  {#if check && profiles.length > 0}
    <div class="check">
      <button
        type="button"
        class="st-btn st-btn--sm"
        disabled={check.checking}
        onclick={() => void check.check(profiles)}
        >{check.checking ? 'Checking…' : 'Check secrets'}</button
      >
      <p class="note">Values are never shown, only whether they resolve.</p>
      {#if check.error}<p class="error" role="alert">{check.error}</p>{/if}
      {#each check.failures as failure (failure)}<p class="error">{failure}</p>{/each}
    </div>
  {/if}
  <form class="add" onsubmit={add}>
    <label for="secret-name">New secret name</label>
    <input
      id="secret-name"
      class="st-input vc-mono"
      autocomplete="off"
      spellcheck="false"
      placeholder="SHOP_TOKEN"
      aria-invalid={problem !== undefined}
      aria-describedby={problem ? 'secret-name-problem' : undefined}
      bind:value={name}
      oninput={() => (problem = undefined)}
    />
    <button type="submit" class="st-btn" disabled={busy || !name.trim()}>Add</button>
    {#if problem}<p id="secret-name-problem" class="error">{problem}</p>{/if}
  </form>
  <!-- the status, then Discard, then the primary last at the right (D616) -->
  <div class="actions">
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
    <button
      type="button"
      class="st-btn"
      disabled={busy || !schema.dirty}
      onclick={() => schema.discard()}>Discard changes</button
    >
    <button
      type="button"
      class="st-btn st-btn--primary"
      disabled={busy || !schema.dirty}
      onclick={() => void schema.save()}>{schema.state === 'saving' ? 'Saving…' : 'Save'}</button
    >
  </div>
</section>

<style>
  .secrets {
    display: grid;
    gap: 12px;
  }
  .intro,
  .empty,
  .note,
  .status {
    margin: 0;
    color: var(--st-text-secondary);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--st-chrome-separator);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    color: var(--st-text-secondary);
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
  .check,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  /* the same width as New profile, with Add beside it (D616) */
  .add input {
    flex: 0 1 320px;
    min-width: 0;
  }
  .actions {
    justify-content: flex-end;
  }
  .actions .status {
    flex: 1 1 200px;
  }
  .add .error,
  .check .error {
    flex-basis: 100%;
  }
  .state.resolved {
    color: var(--vc-pass);
  }
  .state.not-set {
    color: var(--vc-fail);
  }
  .state.not-checked {
    color: var(--st-text-secondary);
  }
  .error {
    margin: 0;
    color: var(--vc-fail);
  }
</style>
