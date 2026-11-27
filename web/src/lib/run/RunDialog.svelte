<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { untrack } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import Icon from '#lib/shell/Icon.svelte';
  import Glyph from '#lib/ui/Glyph.svelte';
  import { planLine, type EstimatePort } from './estimate.ts';
  import {
    emptyForm,
    reportFormats,
    runCommand,
    runRequest,
    selectorParts,
    type ReportFormat,
    type RunForm,
  } from './form.ts';
  import type { RunInit } from './session.svelte.ts';

  /**
   * **Run…** (D632): the selector, profile, concurrency and bail, with a one-line plan of what
   * would run; **More options** holds the layered profiles, changed since, seed, repeat, strict,
   * the report files and the same run as a command to copy. Cancel, then Run, at the bottom right.
   */
  let {
    profiles,
    profile,
    selector = '',
    estimate,
    onrun,
    onclose,
  }: {
    profiles: readonly string[];
    /** The default profile, chosen first. */
    profile: string | undefined;
    selector?: string;
    estimate: EstimatePort;
    onrun: (init: RunInit) => void;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  /** The form starts from the props once; later prop changes do not reset what was typed. */
  let form: RunForm = $state(
    untrack(() => ({ ...emptyForm, selector, profiles: profile ? [profile] : [] })),
  );
  let more = $state(false);
  let plan = $state('Counting…');
  let copied = $state(false);
  const first = $derived(form.profiles[0] ?? '');
  const layers = $derived(form.profiles.slice(1));
  const prod = $derived(form.profiles.some((p) => /prod/i.test(p)));
  const command = $derived(runCommand(form));

  $effect(() => {
    const parts = selectorParts(form.selector);
    const changed = form.changed.trim();
    const timer = setTimeout(() => void count({ ...parts, ...(changed ? { changed } : {}) }), 250);
    return () => clearTimeout(timer);
  });

  async function count(selection: Parameters<EstimatePort>[0]): Promise<void> {
    try {
      plan = planLine(await estimate(selection));
    } catch (error) {
      plan = ConnectError.from(error).rawMessage;
    }
  }

  function setProfile(name: string): void {
    form = { ...form, profiles: name ? [name, ...layers.filter((l) => l !== name)] : [] };
  }

  function toggleLayer(name: string, on: boolean): void {
    const rest = layers.filter((l) => l !== name);
    form = { ...form, profiles: [first, ...(on ? [...rest, name] : rest)].filter(Boolean) };
  }

  function toggleReport(format: ReportFormat, on: boolean): void {
    const rest = form.reports.filter((r) => r !== format);
    form = {
      ...form,
      reports: on ? reportFormats.filter((f) => f === format || rest.includes(f)) : rest,
    };
  }

  const whole = (value: string): number | undefined => {
    const n = Number(value);
    return value.trim() && Number.isInteger(n) && n > 0 ? n : undefined;
  };

  async function copy(): Promise<void> {
    await navigator.clipboard.writeText(command);
    copied = true;
  }
</script>

<Dialog title="Run tests" width={560} {onclose}>
  <div class="form">
    <label for="{uid}-sel">Selector</label>
    <div class="field">
      <input
        id="{uid}-sel"
        class="st-input vc-mono"
        placeholder="All tests"
        aria-describedby="{uid}-sel-hint"
        bind:value={form.selector}
      />
      <span class="hint" id="{uid}-sel-hint"
        >Test or file ID, glob, path or <code>tag:smoke</code>; OR joins them.</span
      >
    </div>
    <label for="{uid}-profile">Profile</label>
    <select
      id="{uid}-profile"
      class="st-input"
      value={first}
      onchange={(event) => setProfile(event.currentTarget.value)}
    >
      {#if !profiles.length}<option value="">Default</option>{/if}
      {#each profiles as name (name)}<option value={name}>{name}</option>{/each}
    </select>
    <label for="{uid}-conc">Concurrency</label>
    <input
      id="{uid}-conc"
      class="st-input num"
      inputmode="numeric"
      placeholder="Config"
      value={form.concurrency ?? ''}
      oninput={(event) => (form = { ...form, concurrency: whole(event.currentTarget.value) })}
    />
    <label for="{uid}-bail">Stop at the first failure</label>
    <input id="{uid}-bail" type="checkbox" class="st-switch" bind:checked={form.bail} />
  </div>
  {#if prod}
    <p class="prod" role="note">
      <strong>Production.</strong> This run sends real requests to {form.profiles.join(', ')}.
    </p>
  {/if}
  <p class="plan st-secondary" aria-live="polite"><span class="vc-sr">Plan: </span>{plan}</p>
  <details class="more" bind:open={more}>
    <summary class="vc-disclosure"><Icon name="chevron" class="vc-chev" />More options</summary>
    <div class="form">
      {#if profiles.length > 1}
        <span class="lbl" id="{uid}-layers">Layer profiles</span>
        <div class="chips" role="group" aria-labelledby="{uid}-layers">
          {#each profiles.filter((p) => p !== first) as name (name)}
            <label class="chip"
              ><input
                type="checkbox"
                class="st-check"
                checked={layers.includes(name)}
                onchange={(event) => toggleLayer(name, event.currentTarget.checked)}
              />{name}</label
            >
          {/each}
        </div>
      {/if}
      <label for="{uid}-changed">Changed since</label>
      <input
        id="{uid}-changed"
        class="st-input vc-mono"
        placeholder="origin/main"
        bind:value={form.changed}
      />
      <label for="{uid}-seed">Seed</label>
      <input
        id="{uid}-seed"
        class="st-input num"
        inputmode="numeric"
        placeholder="Random"
        value={form.seed ?? ''}
        oninput={(event) => (form = { ...form, seed: whole(event.currentTarget.value) })}
      />
      <label for="{uid}-repeat">Repeat</label>
      <input
        id="{uid}-repeat"
        class="st-input num"
        inputmode="numeric"
        placeholder="1"
        value={form.repeat ?? ''}
        oninput={(event) => (form = { ...form, repeat: whole(event.currentTarget.value) })}
      />
      <label for="{uid}-strict">Strict</label>
      <div class="field">
        <input
          id="{uid}-strict"
          type="checkbox"
          class="st-switch"
          aria-describedby="{uid}-strict-hint"
          bind:checked={form.strict}
        />
        <span class="hint" id="{uid}-strict-hint">Allowed failures fail the run.</span>
      </div>
      <span class="lbl" id="{uid}-reports">Report files</span>
      <div class="chips" role="group" aria-labelledby="{uid}-reports">
        {#each reportFormats as format (format)}
          <label class="chip"
            ><input
              type="checkbox"
              class="st-check"
              checked={form.reports.includes(format)}
              onchange={(event) => toggleReport(format, event.currentTarget.checked)}
            />{format}</label
          >
        {/each}
      </div>
    </div>
    <div class="cmd vc-well">
      <code class="vc-mono" aria-label="Command">{command}</code>
      <button
        type="button"
        class="st-btn st-btn--sm"
        onclick={() => void copy()}
        onblur={() => (copied = false)}
        ><Glyph name={copied ? 'check' : 'copy'} />{copied ? 'Copied' : 'Copy'}</button
      >
    </div>
  </details>
  {#snippet actions()}
    <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
    <button type="button" class="st-btn st-btn--primary" onclick={() => onrun(runRequest(form))}
      >Run</button
    >
  {/snippet}
</Dialog>

<style>
  /* one label column for both grids, so More options lines up with the fields above it */
  .form {
    display: grid;
    grid-template-columns: 10.5rem minmax(0, 1fr);
    align-items: center;
    gap: 10px 12px;
  }
  .field {
    display: grid;
    gap: 4px;
  }
  .hint {
    color: var(--st-text-tertiary);
    font: var(--st-type-caption);
  }
  .num {
    width: 96px;
  }
  .st-switch {
    justify-self: start;
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
  .prod {
    margin: 12px 0 0;
    padding: 8px 12px;
    border: 1px solid var(--vc-prod-border);
    border-radius: var(--vc-radius-inner);
    background: var(--vc-prod-bg);
    color: var(--vc-prod-ink);
  }
  .plan {
    margin: 12px 0 0;
    font-variant-numeric: tabular-nums;
  }
  .more {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .more summary {
    margin-bottom: 10px;
    font-weight: 500;
  }
  .cmd {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    padding: 8px 10px;
    border-radius: var(--vc-radius-dialog-inner);
  }
  .cmd code {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
