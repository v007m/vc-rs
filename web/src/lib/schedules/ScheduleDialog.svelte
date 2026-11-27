<script lang="ts">
  import { untrack } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import { lens } from '#lib/ui/segmented.ts';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import { Staged } from '#lib/writes/staged.svelte.ts';
  import { emptyForm, formOf, idProblem, specFrom, type ScheduleForm } from './form.ts';
  import { runnerLabel, runners, type ScheduleSpec } from './port.ts';
  import type { ScheduleList } from './schedules.svelte.ts';
  import { cronWords } from './words.ts';

  /**
   * New schedule, or Edit one (D706): its fields, then **Preview** shows the config diff and
   * **Write** writes it with the preview's digest. Any edit drops the preview. An edited
   * schedule keeps its ID and its `notify` channels, which this form does not change.
   */
  let {
    list,
    profiles,
    editing,
    onclose,
  }: {
    list: ScheduleList;
    /** The profiles the config names. */
    profiles: readonly string[];
    /** The schedule being edited; a new one when absent. */
    editing?: ScheduleSpec;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  const staged = new Staged();
  let form: ScheduleForm = $state(untrack(() => (editing ? formOf(editing) : { ...emptyForm })));
  let tried = $state(false);
  const taken = $derived(editing ? [] : list.lines.map((line) => line.id));
  const problem = $derived(idProblem(form.id.trim(), taken));
  const words = $derived(cronWords(form.cron));

  $effect(() => {
    void JSON.stringify(form);
    untrack(() => staged.reset());
  });

  function toggleProfile(name: string, on: boolean): void {
    const rest = form.profiles.filter((p) => p !== name);
    form.profiles = on ? profiles.filter((p) => p === name || rest.includes(p)) : rest;
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    tried = true;
    if (problem) return;
    const change = { put: specFrom(form, editing?.notify) };
    if (!staged.shown) return list.review(staged, change);
    if (await list.write(staged, change)) onclose();
  }
</script>

<Dialog title={editing ? `Edit ${editing.name ?? editing.id}` : 'New schedule'} {onclose}>
  <form class="sheet" onsubmit={(event) => void submit(event)} novalidate>
    <div class="form">
      <label for="{uid}-id">ID</label>
      <div class="field">
        <input
          id="{uid}-id"
          class="st-input vc-mono"
          autocomplete="off"
          spellcheck="false"
          placeholder="nightlySmoke"
          disabled={editing !== undefined}
          aria-invalid={tried && problem !== undefined}
          aria-describedby="{uid}-id-hint"
          bind:value={form.id}
        />
        <span class="hint" class:bad={tried && problem} id="{uid}-id-hint"
          >{tried && problem ? problem : 'camelCase, unique among the schedules.'}</span
        >
      </div>
      <label for="{uid}-name">Name</label>
      <input
        id="{uid}-name"
        class="st-input"
        autocomplete="off"
        placeholder="Nightly smoke"
        bind:value={form.name}
      />
      <label for="{uid}-cron">Cron</label>
      <div class="field">
        <input
          id="{uid}-cron"
          class="st-input vc-mono"
          autocomplete="off"
          spellcheck="false"
          aria-describedby="{uid}-cron-hint"
          bind:value={form.cron}
        />
        <span class="hint" id="{uid}-cron-hint" aria-live="polite"
          >{words ??
            'Five fields in UTC: minute, hour, day of the month, month, day of the week.'}</span
        >
      </div>
      <label for="{uid}-sel">Selector</label>
      <div class="field">
        <input
          id="{uid}-sel"
          class="st-input vc-mono"
          autocomplete="off"
          spellcheck="false"
          placeholder="All tests"
          aria-describedby="{uid}-sel-hint"
          bind:value={form.selector}
        />
        <span class="hint" id="{uid}-sel-hint"
          >Test or file IDs, globs or paths, separated by spaces.</span
        >
      </div>
      <span class="lbl" id="{uid}-profiles">Profiles</span>
      {#if profiles.length}
        <div class="chips" role="group" aria-labelledby="{uid}-profiles">
          {#each profiles as name (name)}
            <label class="chip"
              ><input
                type="checkbox"
                class="st-check"
                checked={form.profiles.includes(name)}
                onchange={(event) => toggleProfile(name, event.currentTarget.checked)}
              />{name}</label
            >
          {/each}
        </div>
      {:else}
        <span class="st-secondary">The default profiles</span>
      {/if}
      <span class="lbl" id="{uid}-runner">Runs on</span>
      <div class="st-segmented" role="group" aria-labelledby="{uid}-runner" {@attach lens}>
        {#each runners as runner (runner)}
          <button
            type="button"
            aria-pressed={form.runner === runner}
            onclick={() => (form.runner = runner)}>{runnerLabel[runner]}</button
          >
        {/each}
      </div>
      <label for="{uid}-on">On</label>
      <input id="{uid}-on" type="checkbox" class="st-switch" bind:checked={form.enabled} />
    </div>
    {#if staged.note}<p class="note" role="status">{staged.note}</p>{/if}
    {#if staged.shown}
      <div class="diff"><ChangeList files={staged.shown.files} /></div>
    {/if}
    <div class="actions">
      <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="st-btn st-btn--primary" disabled={staged.busy}
        >{staged.shown ? 'Write' : 'Preview'}</button
      >
    </div>
  </form>
</Dialog>

<style>
  .form {
    display: grid;
    grid-template-columns: 7rem minmax(0, 1fr);
    align-items: center;
    gap: 10px 12px;
  }
  .field {
    display: grid;
    gap: 4px;
  }
  .hint {
    color: var(--st-text-secondary);
    font: var(--st-type-caption);
  }
  .hint.bad {
    color: var(--vc-fail-ink);
  }
  .lbl {
    align-self: start;
    padding-top: 2px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .st-segmented,
  .st-switch {
    justify-self: start;
  }
  .note {
    margin: 12px 0 0;
    color: var(--st-text-secondary);
  }
  .diff {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
