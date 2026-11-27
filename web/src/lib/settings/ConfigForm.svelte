<script lang="ts">
  import { FIELDS } from './configEdits.ts';
  import type { ConfigForm } from './configForm.svelte.ts';
  import { play } from '#lib/ui/symbols.ts';
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
  let profileInput: HTMLInputElement | undefined = $state();
  let varInput: HTMLInputElement | undefined = $state();
  let status: HTMLElement | undefined = $state();

  /** A refused add shakes the field it came from (Strata 1.1 attention wiggle). */
  function refuse(input: HTMLInputElement | undefined, error: string | undefined): void {
    if (error !== undefined && input) play(input, 'wiggle');
  }

  /** A clean save bounces its confirmation once (Strata 1.1 confirm bounce). */
  async function save(): Promise<void> {
    await form.save();
    if (form.saved && !form.dirty && status) play(status, 'bounce');
  }

  function addProfile(event: SubmitEvent): void {
    event.preventDefault();
    profileError = form.addProfile(profile.trim());
    refuse(profileInput, profileError);
    if (profileError === undefined) profile = '';
  }

  function addVar(event: SubmitEvent): void {
    event.preventDefault();
    varError = form.addVar(varName.trim(), varValue);
    refuse(varInput, varError);
    if (varError === undefined) [varName, varValue] = ['', ''];
  }
</script>

<section class="form" aria-labelledby="config-h" aria-busy={busy}>
  <h2 id="config-h" class="st-title3">Project config</h2>
  <p class="intro">
    What <code>vibranium.config.yaml</code> itself sets; profiles and <code>.env</code> may override it.
    Saving edits the file in place and keeps its comments.
  </p>
  {#if form.error}
    <p class="error" role="alert">{form.error}</p>
  {/if}
  {#if form.state === 'conflict'}
    <div class="vc-banner" role="alert">
      <span class="vc-grow"
        >Not saved: <code>vibranium.config.yaml</code> changed on disk since you opened it.</span
      >
      <button type="button" class="st-btn st-btn--sm" onclick={() => void form.load()}
        >Reload from disk</button
      >
    </div>
  {/if}

  <fieldset>
    <legend>General</legend>
    <label class="field">
      <span>Project</span>
      <input class="st-input" autocomplete="off" disabled={busy} bind:value={form.draft.project} />
    </label>
    {#each FIELDS as field (field.key)}
      <label class="field">
        <span>{field.label}</span>
        <input
          class="st-input vc-mono"
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
          <label class="pick">
            <input
              type="radio"
              class="vc-radio"
              name="default-profile"
              checked={entry.isDefault}
              disabled={busy}
              onchange={() => form.makeDefault(entry.id)}
            />
            <span class="vc-mono">{entry.id}</span>
            {#if entry.isDefault}<span class="tag">default</span>{/if}
          </label>
          <button
            type="button"
            class="st-btn st-btn--ghost st-btn--sm vc-btn--row-danger"
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
        class="st-input vc-mono"
        bind:this={profileInput}
        autocomplete="off"
        spellcheck="false"
        placeholder="staging"
        aria-invalid={profileError !== undefined}
        aria-describedby={profileError ? 'profile-id-problem' : undefined}
        bind:value={profile}
        oninput={() => (profileError = undefined)}
      />
      <button type="submit" class="st-btn" disabled={busy || !profile.trim()}>Add</button>
      {#if profileError}<p id="profile-id-problem" class="error">{profileError}</p>{/if}
    </form>
  </fieldset>

  <fieldset>
    <legend>Variables</legend>
    <ul class="rows">
      {#each Object.keys(form.draft.vars) as name (name)}
        <li>
          <label class="var">
            <span class="vc-mono">{name}</span>
            {#if form.draft.vars[name]?.includes(MASKED)}
              <span class="masked"
                >{form.draft.vars[name]}<span class="vc-sr">
                  (reads a secret; change it in the editor)</span
                ></span
              >
            {:else}
              <input
                class="st-input vc-mono"
                autocomplete="off"
                spellcheck="false"
                disabled={busy}
                bind:value={form.draft.vars[name]}
              />
              {#if form.draft.resolved.includes(name)}
                <span class="tag"
                  >resolved<span class="vc-sr">
                    (shown with its references filled in; a change is written as typed)</span
                  ></span
                >
              {/if}
            {/if}
          </label>
          <button
            type="button"
            class="st-btn st-btn--ghost st-btn--sm vc-btn--row-danger"
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
        class="st-input vc-mono"
        bind:this={varInput}
        autocomplete="off"
        spellcheck="false"
        placeholder="ordersUrl"
        aria-invalid={varError !== undefined}
        aria-describedby={varError ? 'var-name-problem' : undefined}
        bind:value={varName}
        oninput={() => (varError = undefined)}
      />
      <label for="var-value" class="vc-sr">Value</label>
      <input
        id="var-value"
        class="st-input vc-mono"
        autocomplete="off"
        spellcheck="false"
        placeholder="http://localhost:8081"
        bind:value={varValue}
      />
      <button type="submit" class="st-btn" disabled={busy || !varName.trim()}>Add</button>
      {#if varError}<p id="var-name-problem" class="error">{varError}</p>{/if}
    </form>
  </fieldset>

  {#if form.diff !== undefined}
    <pre class="diff vc-mono" aria-label="Changes to vibranium.config.yaml">{form.diff ||
        'No change to the file.'}</pre>
  {/if}
  <!-- the section's footer: Cancel-like actions first, the primary last at the right (D616) -->
  <div class="actions">
    <p class="status" role="status" bind:this={status}>
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
    <button
      type="button"
      class="st-btn"
      disabled={busy || !form.dirty}
      onclick={() => void form.review()}
      >{form.state === 'reviewing' ? 'Reviewing…' : 'Review changes'}</button
    >
    <button
      type="button"
      class="st-btn"
      disabled={busy || !form.dirty}
      onclick={() => form.discard()}>Discard changes</button
    >
    <button
      type="button"
      class="st-btn st-btn--primary"
      disabled={busy || !form.dirty}
      onclick={() => void save()}>{form.state === 'saving' ? 'Saving…' : 'Save'}</button
    >
  </div>
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
    color: var(--st-text-secondary);
  }
  /* Settings rows (D603): a group is a section label over left-aligned rows, no box; the rule
     between two rows is inset by the same 12px at both ends. */
  fieldset {
    display: grid;
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    padding: 0 0 4px;
    font: var(--st-type-label);
    letter-spacing: var(--st-tracking-label);
    color: var(--st-text-tertiary);
  }
  .field,
  .rows li,
  .add {
    position: relative;
    min-height: 44px;
    padding: 7px 12px;
  }
  .field + .field::before,
  .rows + .add::before,
  .rows li + li::before {
    content: '';
    position: absolute;
    inset: 0 12px auto;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .rows {
    position: relative;
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  /* fields keep their own width (at most 320px) and never stretch across the card */
  .field {
    display: grid;
    grid-template-columns: 120px minmax(0, 320px);
    gap: 8px;
    align-items: center;
    color: var(--st-text-secondary);
  }
  .rows li {
    justify-content: flex-start;
  }
  .add input {
    flex: 0 1 200px;
    min-width: 0;
  }
  .add #profile-id {
    flex-basis: 320px;
  }
  .var .st-input {
    flex: 0 1 320px;
    min-width: 0;
  }
  .rows li,
  .rows label,
  .add,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .vc-grow {
    flex: 1;
  }
  .pick,
  .var {
    min-width: 0;
  }
  /* The section's footer, on the card's own surface: no fill, a hairline that stops 12px short
     of both ends (D614), Cancel-like actions first and Save last (D616). It stays in the flow:
     Secrets and Auth follow, so a sticky footer would always cover the rows scrolling under
     it, and a translucent one either shows them through or reads as a panel. */
  .actions {
    position: relative;
    justify-content: flex-end;
    margin: 0 -12px;
    padding: 12px 12px 0;
  }
  .actions::before {
    content: '';
    position: absolute;
    inset: 0 12px auto;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .actions .status {
    flex: 1 1 200px;
  }
  .var .vc-mono:first-child {
    min-width: 120px;
  }
  .tag {
    font-size: 12px;
    color: var(--st-text-secondary);
  }
  .masked {
    letter-spacing: 1px;
  }
  .add .error {
    flex-basis: 100%;
  }
  .diff {
    margin: 0;
    padding: 8px 12px;
    overflow-x: auto;
    border-radius: var(--st-radius-md);
    background: var(--st-surface-wash);
    white-space: pre;
  }
  .error {
    margin: 0;
    color: var(--vc-fail);
  }
</style>
