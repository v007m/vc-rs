<script lang="ts">
  import { FIELDS } from './configEdits.ts';
  import type { ConfigForm } from './configForm.svelte.ts';
  import { MASKED } from './port.ts';

  /**
   * `vibranium.config.yaml` as a form: the project's name and defaults, its profiles and its
   * vars. Edits go to the file by pointer, keeping its comments, after a masked diff review.
   */
  let { form }: { form: ConfigForm } = $props();

  let profile = $state('');
  let profileError: string | undefined = $state();
  let varName = $state('');
  let varValue = $state('');
  let varError: string | undefined = $state();
  let busy = $derived(form.state !== 'idle' && form.state !== 'conflict');

  function addProfile(event: SubmitEvent): void {
    event.preventDefault();
    profileError = form.addProfile(profile.trim());
    if (profileError === undefined) profile = '';
  }

  function addVar(event: SubmitEvent): void {
    event.preventDefault();
    varError = form.addVar(varName.trim(), varValue);
    if (varError === undefined) [varName, varValue] = ['', ''];
  }
</script>

<section class="form" aria-labelledby="config-h" aria-busy={busy}>
  <h2 id="config-h" class="h2">Project config</h2>
  <p class="intro">
    What <code>vibranium.config.yaml</code> itself sets; profiles and <code>.env</code> may override it.
    Saving edits the file in place and keeps its comments.
  </p>
  {#if form.error}
    <p class="error" role="alert">{form.error}</p>
  {/if}
  {#if form.state === 'conflict'}
    <div class="banner" role="alert">
      <span class="grow"
        >Not saved: <code>vibranium.config.yaml</code> changed on disk since you opened it.</span
      >
      <button type="button" class="btn sm" onclick={() => void form.load()}>Reload from disk</button
      >
    </div>
  {/if}

  <fieldset>
    <legend>General</legend>
    <label class="field">
      <span>Project</span>
      <input class="input" autocomplete="off" disabled={busy} bind:value={form.draft.project} />
    </label>
    {#each FIELDS as field (field.key)}
      <label class="field">
        <span>{field.label}</span>
        <input
          class="input mono"
          autocomplete="off"
          inputmode={field.kind === 'number' ? 'numeric' : 'text'}
          placeholder={field.hint}
          disabled={busy}
          bind:value={form.draft.fields[field.key]}
        />
      </label>
    {/each}
  </fieldset>

  <fieldset>
    <legend>Profiles</legend>
    <ul class="rows">
      {#each form.draft.profiles as entry (entry.id)}
        <li>
          <label class="grow">
            <input
              type="radio"
              name="default-profile"
              checked={entry.isDefault}
              disabled={busy}
              onchange={() => form.makeDefault(entry.id)}
            />
            <span class="mono">{entry.id}</span>
            {#if entry.isDefault}<span class="tag">default</span>{/if}
          </label>
          <button
            type="button"
            class="btn sm"
            aria-label="Remove profile {entry.id}"
            disabled={busy}
            onclick={() => form.removeProfile(entry.id)}>Remove</button
          >
        </li>
      {:else}
        <li class="empty">{form.state === 'loading' ? 'Loading…' : 'No profiles yet.'}</li>
      {/each}
    </ul>
    <form class="add" onsubmit={addProfile}>
      <label for="profile-id">New profile</label>
      <input
        id="profile-id"
        class="input mono"
        autocomplete="off"
        spellcheck="false"
        placeholder="staging"
        aria-invalid={profileError !== undefined}
        aria-describedby={profileError ? 'profile-id-problem' : undefined}
        bind:value={profile}
        oninput={() => (profileError = undefined)}
      />
      <button type="submit" class="btn sm" disabled={busy || !profile.trim()}>Add</button>
      {#if profileError}<p id="profile-id-problem" class="error">{profileError}</p>{/if}
    </form>
  </fieldset>

  <fieldset>
    <legend>Variables</legend>
    <ul class="rows">
      {#each Object.keys(form.draft.vars) as name (name)}
        <li>
          <label class="var grow">
            <span class="mono">{name}</span>
            {#if form.draft.vars[name] === MASKED}
              <span class="masked"
                >{MASKED}<span class="sr"> (reads a secret; change it in the editor)</span></span
              >
            {:else}
              <input
                class="input mono"
                autocomplete="off"
                spellcheck="false"
                disabled={busy}
                bind:value={form.draft.vars[name]}
              />
            {/if}
          </label>
          <button
            type="button"
            class="btn sm"
            aria-label="Remove variable {name}"
            disabled={busy}
            onclick={() => form.removeVar(name)}>Remove</button
          >
        </li>
      {:else}
        <li class="empty">{form.state === 'loading' ? 'Loading…' : 'No variables yet.'}</li>
      {/each}
    </ul>
    <form class="add" onsubmit={addVar}>
      <label for="var-name">New variable</label>
      <input
        id="var-name"
        class="input mono"
        autocomplete="off"
        spellcheck="false"
        placeholder="ordersUrl"
        aria-invalid={varError !== undefined}
        aria-describedby={varError ? 'var-name-problem' : undefined}
        bind:value={varName}
        oninput={() => (varError = undefined)}
      />
      <label for="var-value" class="sr">Value</label>
      <input
        id="var-value"
        class="input mono"
        autocomplete="off"
        spellcheck="false"
        placeholder="http://localhost:8081"
        bind:value={varValue}
      />
      <button type="submit" class="btn sm" disabled={busy || !varName.trim()}>Add</button>
      {#if varError}<p id="var-name-problem" class="error">{varError}</p>{/if}
    </form>
  </fieldset>

  <div class="actions">
    <button
      type="button"
      class="btn"
      disabled={busy || !form.dirty}
      onclick={() => void form.review()}
      >{form.state === 'reviewing' ? 'Reviewing…' : 'Review changes'}</button
    >
    <button
      type="button"
      class="btn primary"
      disabled={busy || !form.dirty}
      onclick={() => void form.save()}>{form.state === 'saving' ? 'Saving…' : 'Save'}</button
    >
    <button type="button" class="btn" disabled={busy || !form.dirty} onclick={() => form.discard()}
      >Discard changes</button
    >
    <p class="status" role="status">
      {#if form.dirty}
        {form.edits.length} unsaved change{form.edits.length === 1 ? '' : 's'}.
      {:else if form.saved?.checkError}
        Saved, but the project could not be checked: {form.saved.checkError}
      {:else if form.saved}
        Saved. {form.saved.problems === 0
          ? 'The project checks clean.'
          : `The project has ${form.saved.problems} problem${form.saved.problems === 1 ? '' : 's'}; see them in the editor.`}
      {/if}
    </p>
  </div>
  {#if form.diff !== undefined}
    <pre class="diff mono" aria-label="Changes to vibranium.config.yaml">{form.diff ||
        'No change to the file.'}</pre>
  {/if}
</section>

<style>
  .form {
    display: grid;
    gap: 12px;
  }
  .intro,
  .empty,
  .status {
    margin: 0;
    color: var(--ink2);
  }
  fieldset {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  legend {
    padding: 0 4px;
    font-weight: 500;
  }
  .field {
    display: grid;
    grid-template-columns: 120px minmax(0, 320px);
    gap: 8px;
    align-items: center;
    color: var(--ink2);
  }
  .rows {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rows li,
  .rows label,
  .add,
  .actions,
  .banner {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .var .mono:first-child {
    min-width: 120px;
  }
  .tag {
    font-size: 12px;
    color: var(--ink2);
  }
  .masked {
    letter-spacing: 1px;
  }
  .add .error {
    flex-basis: 100%;
  }
  .banner {
    padding: 8px 12px;
    border: 1px solid var(--fail);
    border-radius: 6px;
  }
  .diff {
    margin: 0;
    padding: 8px 12px;
    overflow-x: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
    white-space: pre;
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
</style>
