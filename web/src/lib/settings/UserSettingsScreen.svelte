<script lang="ts">
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import { getToasts, type Toasts } from '#lib/ui/toasts.svelte.ts';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import { MASKED } from './port.ts';
  import type { UserChange, UserRow } from './userKeys.ts';
  import type { UserSettings } from './userSettings.svelte.ts';

  /** Your settings (D711): the user config's keys, edits staged, previewed, then saved. */
  let { settings, toasts = getToasts() }: { settings: UserSettings; toasts?: Toasts } = $props();

  const uid = $props.id();
  let key = $state('');
  let value = $state('');
  /** Whether the key being set holds a masked value now, which is replaced, never shown. */
  let replacing = $state(false);
  let problem: string | undefined = $state();
  let valueField: HTMLInputElement | undefined = $state();
  let staged = $derived(settings.staged);
  let count = $derived(settings.edits.length);

  const changes: Readonly<Record<UserChange, [string, string]>> = {
    added: ['New', 'vc-pass'],
    edited: ['Changed', 'vc-accent'],
    removed: ['Removed', 'vc-fail'],
  };

  function edit(row: UserRow): void {
    key = row.key;
    replacing = row.masked;
    value = row.masked ? '' : row.value;
    problem = undefined;
    valueField?.focus();
  }

  /** A new key forgets the problem with the old one, and that its value was masked. */
  function rekey(): void {
    problem = undefined;
    replacing = false;
  }

  function set(event: SubmitEvent): void {
    event.preventDefault();
    problem = settings.set(key.trim(), value);
    if (problem !== undefined) return;
    key = '';
    value = '';
    replacing = false;
  }

  async function save(): Promise<void> {
    if (await settings.write()) toasts.show('Saved your settings.');
    else if (staged.note) toasts.show(staged.note, 'error');
  }
</script>

{#if settings.missing}
  <EmptyState icon="gear" title="Your settings are not available" caption={settings.missing} />
{:else}
  <section class="user" aria-labelledby="{uid}-title">
    <h2 id="{uid}-title" class="st-title3">Your settings</h2>
    <p class="intro">
      Your own settings, for every project on this machine. Where this project's config sets the
      same key, the project's value wins.
    </p>
    {#if settings.listing}
      <p class="path">File <code class="vc-mono">{settings.listing.path}</code></p>
    {/if}
    {#if settings.error}<p class="error" role="alert">{settings.error}</p>{/if}
    <table class="vc-table" aria-busy={staged.busy}>
      <caption class="vc-sr">Your settings</caption>
      <thead>
        <tr>
          <th scope="col">Key</th>
          <th scope="col">Value</th>
          <th scope="col"><span class="vc-sr">Actions</span></th>
        </tr>
      </thead>
      <tbody>
        {#each settings.rows as row (row.key)}
          <tr>
            <th scope="row">
              <span class="vc-mono">{row.key}</span>
              {#if row.shadowed}
                <span
                  class="st-badge"
                  title="This project's config sets {row.key} too, so the project's value wins here."
                  >Shadowed by the project</span
                >
              {/if}
            </th>
            <td class:gone={row.change === 'removed'}>
              {#if row.masked}
                <span class="vc-mono">{MASKED}</span>
                <span class="vc-sr">(secret, masked)</span>
              {:else}
                <span class="vc-mono">{row.value}</span>
              {/if}
              {#if row.change}
                {@const [label, tone] = changes[row.change]}
                <span class="st-badge {tone}">{label}</span>
              {/if}
            </td>
            <td class="act">
              {#if row.change}
                <button
                  type="button"
                  class="st-btn st-btn--ghost st-btn--sm"
                  aria-label="Undo the change to {row.key}"
                  onclick={() => settings.undo(row.key)}>Undo</button
                >
              {:else}
                <button
                  type="button"
                  class="st-btn st-btn--ghost st-btn--sm"
                  aria-label="{row.masked ? 'Replace' : 'Edit'} {row.key}"
                  onclick={() => edit(row)}>{row.masked ? 'Replace' : 'Edit'}</button
                >
                <button
                  type="button"
                  class="st-btn st-btn--ghost st-btn--sm vc-btn--row-danger"
                  aria-label="Remove {row.key}"
                  onclick={() => settings.remove(row.key)}>Remove</button
                >
              {/if}
            </td>
          </tr>
        {:else}
          <tr>
            <td colspan="3" class="empty"
              >{settings.listing ? 'Your user config sets nothing yet.' : 'Loading…'}</td
            >
          </tr>
        {/each}
      </tbody>
    </table>

    <form class="add" onsubmit={set}>
      <label class="field">
        <span>Key</span>
        <input
          class="st-input vc-mono"
          autocomplete="off"
          spellcheck="false"
          placeholder="defaults.timeout"
          aria-invalid={problem !== undefined}
          aria-describedby={problem ? `${uid}-problem` : `${uid}-keys`}
          bind:value={key}
          oninput={rekey}
        />
      </label>
      <label class="field">
        <span>Value</span>
        <input
          class="st-input vc-mono"
          autocomplete="off"
          spellcheck="false"
          placeholder={replacing ? 'A new value; the current one stays hidden' : '12s'}
          bind:this={valueField}
          bind:value
        />
      </label>
      <button type="submit" class="st-btn" disabled={!key.trim()}>Set</button>
      {#if problem}
        <p id="{uid}-problem" class="error">{problem}</p>
      {:else}
        <p id="{uid}-keys" class="hint">
          You can set profile, defaults.&lt;name&gt;, reports.&lt;name&gt; and vars.&lt;name&gt;.
        </p>
      {/if}
    </form>

    <div class="actions">
      <p class="status" role="status">
        {#if count > 0}{count} unsaved {count === 1 ? 'change' : 'changes'}.{/if}
      </p>
      <button
        type="button"
        class="st-btn"
        disabled={staged.busy || count === 0}
        onclick={() => settings.discard()}>Discard changes</button
      >
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={staged.busy || count === 0}
        onclick={() => void settings.preview()}>Preview</button
      >
    </div>
    {#if staged.note}<p class="error" role="alert">{staged.note}</p>{/if}

    {#if staged.shown}
      <section class="preview" aria-label="Preview of your settings">
        <h3 class="st-title3">Preview</h3>
        <ChangeList files={staged.shown.files} />
        <div class="actions">
          <button
            type="button"
            class="st-btn st-btn--primary"
            disabled={staged.busy}
            onclick={() => void save()}>{staged.busy ? 'Saving…' : 'Save'}</button
          >
        </div>
      </section>
    {/if}
  </section>
{/if}

<style>
  .user,
  .preview {
    display: grid;
    gap: 12px;
  }
  .intro,
  .path,
  .empty,
  .hint,
  .status {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .path code {
    color: var(--st-text-primary);
  }
  .vc-table th .st-badge,
  .vc-table td .st-badge {
    margin-left: 6px;
  }
  .gone .vc-mono {
    text-decoration: line-through;
    color: var(--st-text-secondary);
  }
  .act {
    width: 1%;
    white-space: nowrap;
  }
  .add,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: flex-end;
  }
  .field {
    display: grid;
    gap: 4px;
    flex: 0 1 260px;
    min-width: 0;
  }
  .field span {
    font: var(--st-type-caption);
    color: var(--st-text-secondary);
  }
  .add .error,
  .add .hint {
    flex-basis: 100%;
  }
  .actions {
    justify-content: flex-end;
    align-items: center;
  }
  .actions .status {
    flex: 1 1 200px;
  }
  .error {
    margin: 0;
    color: var(--vc-fail-ink);
  }
</style>
