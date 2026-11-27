<script lang="ts">
  import type { Toasts } from '#lib/ui/toasts.svelte.ts';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import { Staged } from '#lib/writes/staged.svelte.ts';
  import { folderName, type OnboardingPort } from './port.ts';
  import StepNote from './StepNote.svelte';
  import { finish, loud, type Reload } from './steps.ts';

  /**
   * `vc init` as a form (D716): every answer is optional, Preview shows each file it would write,
   * and Write sends the preview's digest back, so nothing is written unseen.
   */
  let { port, project, toasts }: { port: OnboardingPort; project: Reload; toasts: Toasts } =
    $props();

  const staged = new Staged();
  let choices = $state({
    name: '',
    service: '',
    baseUrl: '',
    profile: '',
    sample: true,
  });
  let root = $state('');

  const fields = [
    { key: 'name', label: 'Project name', hint: 'This folder’s name', mono: false },
    { key: 'service', label: 'First service', hint: 'users', mono: true },
    { key: 'baseUrl', label: 'Base URL', hint: 'http://localhost:8080', mono: true },
    { key: 'profile', label: 'Default profile', hint: 'default', mono: true },
  ] as const;

  /** An edit makes the preview stale, so it goes. */
  function edited(): void {
    if (staged.shown) staged.reset();
  }

  async function review(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const asked = $state.snapshot(choices);
    await staged.review(
      loud(toasts, async () => {
        const got = await port.init(asked);
        if (got.available) root = got.value.root;
        return got;
      }),
    );
  }

  async function write(): Promise<void> {
    const asked = $state.snapshot(choices);
    const wrote = await staged.write(loud(toasts, (digest: string) => port.init(asked, digest)));
    const name = asked.name.trim() || folderName(root);
    if (wrote) await finish(toasts, project, { text: `Set up ${name} in ${root}` });
  }
</script>

<form class="wizard" aria-label="Set up this folder" oninput={edited} onsubmit={review}>
  <div class="rows">
    {#each fields as field (field.key)}
      <label class="field">
        <span>{field.label}</span>
        <input
          class="st-input"
          class:vc-mono={field.mono}
          autocomplete="off"
          spellcheck="false"
          placeholder={field.hint}
          disabled={staged.busy}
          bind:value={choices[field.key]}
        />
      </label>
    {/each}
    <label class="field check">
      <input
        type="checkbox"
        class="st-check"
        disabled={staged.busy}
        bind:checked={choices.sample}
      />
      <span>Add a sample test</span>
    </label>
  </div>
  <StepNote note={staged.note} />
  {#if staged.shown}
    <p class="lead">Files go in <span class="vc-mono">{root}</span>.</p>
    <ChangeList files={staged.shown.files} />
  {/if}
  <div class="actions">
    {#if staged.shown}
      <button type="button" class="st-btn" disabled={staged.busy} onclick={() => staged.reset()}
        >Back</button
      >
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={staged.busy}
        onclick={() => void write()}>{staged.busy ? 'Writing…' : 'Write files'}</button
      >
    {:else}
      <button type="submit" class="st-btn st-btn--primary" disabled={staged.busy}
        >{staged.busy ? 'Previewing…' : 'Preview'}</button
      >
    {/if}
  </div>
</form>

<style>
  .wizard {
    display: grid;
    gap: 12px;
  }
  .lead {
    margin: 0;
    color: var(--st-text-secondary);
  }
  /* settings rows (D603): left-aligned rows parted by hairlines inset 12px at both ends */
  .rows {
    display: grid;
  }
  .field {
    position: relative;
    display: grid;
    grid-template-columns: 140px minmax(0, 320px);
    gap: 8px;
    align-items: center;
    min-height: 44px;
    padding: 7px 12px;
    color: var(--st-text-secondary);
  }
  .field + .field::before {
    content: '';
    position: absolute;
    inset: 0 12px auto;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .check {
    display: flex;
    gap: 8px;
    color: var(--st-text-primary);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
