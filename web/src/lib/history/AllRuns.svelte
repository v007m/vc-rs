<script lang="ts">
  import { goto } from '$app/navigation';
  import Menu from '#lib/ui/Menu.svelte';
  import { lens } from '#lib/ui/segmented.ts';
  import { distinct, type RunHistory } from './history.svelte.ts';
  import { compareHref, lineCommand } from './links.ts';
  import { outcomeLabel } from './outcome.ts';
  import type { RunLine, RunOutcome } from './port.ts';
  import RunsTable from './RunsTable.svelte';

  /**
   * All runs (D631): words to look for, the kind, status, profile and branch, then the table.
   * Ticking two runs offers **Compare**; each row's ⋯ has Compare with previous, Run again and
   * Copy run command.
   */
  let { history, onrun }: { history: RunHistory; onrun: (selector: string) => void } = $props();

  const kinds = [
    { id: undefined, label: 'All' },
    { id: 'run', label: 'Tests' },
    { id: 'load', label: 'Load' },
    { id: 'adhoc', label: 'Ad hoc' },
  ] as const;
  const outcomes: readonly RunOutcome[] = ['passed', 'failed', 'cancelled', 'unfinished'];
  let picked: readonly number[] = $state([]);
  let note: string | undefined = $state();
  const profiles = $derived(distinct(history.runs, (run) => run.profiles));
  const branches = $derived(distinct(history.runs, (run) => [run.branch ?? '']));
  const pair = $derived([...picked].sort((a, b) => a - b));

  function pick(id: string, all: string): string | undefined {
    return id === all ? undefined : id;
  }

  function toggle(id: number, on: boolean): void {
    picked = on
      ? [...picked.filter((p) => p !== id), id].slice(-2)
      : picked.filter((p) => p !== id);
  }

  async function act(run: RunLine, action: string): Promise<void> {
    const before = history.previous(run.id);
    if (action === 'compare' && before) await goto(compareHref(before.id, run.id));
    if (action === 'again') onrun(run.selector);
    if (action === 'copy') {
      await navigator.clipboard.writeText(lineCommand(run));
      note = `Copied the command for run #${run.id}.`;
    }
  }

  function actions(run: RunLine) {
    const before = history.previous(run.id);
    return [
      {
        id: 'compare',
        label: before ? `Compare with #${before.id}` : 'Compare with previous',
        disabled: !before,
      },
      { id: 'again', label: 'Run again…', disabled: run.kind !== 'run' },
      { id: 'copy', label: 'Copy run command' },
    ];
  }
</script>

<div class="bar" role="search">
  <input
    class="st-input search"
    type="search"
    placeholder="Search runs"
    aria-label="Search runs by number, selector, profile or branch"
    value={history.filter.query ?? ''}
    oninput={(event) => (history.filter = { ...history.filter, query: event.currentTarget.value })}
  />
  <div class="st-segmented" role="group" aria-label="Kind" {@attach lens}>
    {#each kinds as kind (kind.label)}
      <button
        type="button"
        aria-pressed={history.scope.kind === kind.id}
        onclick={() => void history.rescope({ ...history.scope, kind: kind.id })}
        >{kind.label}</button
      >
    {/each}
  </div>
  <Menu
    label="Status"
    items={[
      { id: 'any', label: 'Any status' },
      ...outcomes.map((o) => ({ id: o, label: outcomeLabel[o] })),
    ]}
    onpick={(id) =>
      (history.filter = { ...history.filter, outcome: pick(id, 'any') as RunOutcome | undefined })}
  >
    Status: {history.filter.outcome ? outcomeLabel[history.filter.outcome] : 'Any'}
  </Menu>
  <Menu
    label="Profile"
    items={[{ id: '', label: 'Any profile' }, ...profiles.map((p) => ({ id: p, label: p }))]}
    onpick={(id) => void history.rescope({ ...history.scope, profile: pick(id, '') })}
  >
    Profile: {history.scope.profile ?? 'Any'}
  </Menu>
  <Menu
    label="Branch"
    items={[{ id: '', label: 'Any branch' }, ...branches.map((b) => ({ id: b, label: b }))]}
    onpick={(id) => void history.rescope({ ...history.scope, branch: pick(id, '') })}
  >
    Branch: {history.scope.branch ?? 'Any'}
  </Menu>
  <span class="vc-grow"></span>
  <span class="st-secondary count" aria-live="polite">
    {picked.length ? `${picked.length} of 2 picked` : `${history.shown.length} runs`}
  </span>
  <button
    type="button"
    class="st-btn st-btn--sm"
    disabled={pair.length < 2}
    title="Tick two runs to compare them"
    onclick={() => void goto(compareHref(pair[0] ?? 0, pair[1] ?? 0))}>Compare</button
  >
</div>
{#if history.error}<p class="error" role="alert">{history.error}</p>{/if}
{#if note}<p class="vc-sr" role="status">{note}</p>{/if}
<RunsTable runs={history.shown} {picked} ontoggle={toggle}>
  {#snippet menu(run)}
    <Menu
      label="Run #{run.id} actions"
      button="st-icon-btn"
      items={actions(run)}
      onpick={(id) => void act(run, id)}><span aria-hidden="true">⋯</span></Menu
    >
  {/snippet}
</RunsTable>

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .search {
    width: 220px;
  }
  .count {
    font-variant-numeric: tabular-nums;
  }
  .error {
    margin: 0 0 8px;
    color: var(--vc-fail-ink);
  }
</style>
