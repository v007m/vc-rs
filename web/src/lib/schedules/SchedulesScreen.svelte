<script lang="ts">
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import type { ScheduleLine } from './port.ts';
  import ScheduleDialog from './ScheduleDialog.svelte';
  import { specOf, type ScheduleList } from './schedules.svelte.ts';
  import SchedulesTable from './SchedulesTable.svelte';
  import WriteDialog from './WriteDialog.svelte';

  /**
   * Schedules (D520, ui.md): runs that start on a cron while `vc ui` is up, kept in
   * `vibranium.config.yaml`. **New schedule** and each row's Edit, Export and Remove write the
   * config or a workflow through a previewed diff.
   */
  let { list, profiles }: { list: ScheduleList; profiles: readonly string[] } = $props();

  type Open =
    { kind: 'new' } | { kind: 'edit' | 'export' | 'remove'; line: ScheduleLine } | undefined;
  let open: Open = $state.raw();
  const count = $derived(list.lines.length);

  function pick(line: ScheduleLine, action: string): void {
    if (action === 'edit' || action === 'export' || action === 'remove')
      open = { kind: action, line };
  }

  const close = () => (open = undefined);
</script>

{#snippet create()}
  <button type="button" class="st-btn st-btn--primary" onclick={() => (open = { kind: 'new' })}
    >New schedule</button
  >
{/snippet}

{#if list.reason}
  <EmptyState icon="history" title="Schedules are not available" caption={list.reason} />
{:else if list.loaded && count === 0 && !list.error}
  <EmptyState
    icon="history"
    title="No schedules yet"
    caption="A schedule runs tests on a cron while vc ui is up, and is kept in vibranium.config.yaml."
    action={create}
  />
{:else}
  <div class="bar">
    <p class="st-secondary intro">
      Runs that start on a cron, in UTC, while <code>vc ui</code> is up. They are kept in
      <code>vibranium.config.yaml</code>.
    </p>
    <span class="st-secondary count" aria-live="polite"
      >{count} {count === 1 ? 'schedule' : 'schedules'}</span
    >
    {@render create()}
  </div>
  {#if list.error}<p class="error" role="alert">{list.error}</p>{/if}
  {#if count}<SchedulesTable {list} onpick={pick} />{/if}
{/if}

{#if open?.kind === 'new'}
  <ScheduleDialog {list} {profiles} onclose={close} />
{:else if open?.kind === 'edit'}
  <ScheduleDialog {list} {profiles} editing={specOf(open.line)} onclose={close} />
{:else if open?.kind === 'remove'}
  {@const id = open.line.id}
  <WriteDialog
    title="Remove {open.line.name ?? id}"
    intro="This takes the schedule out of vibranium.config.yaml. Its past runs stay in the history."
    action="Remove"
    danger
    onreview={(staged) => list.review(staged, { remove: id })}
    onwrite={async (staged) => {
      await list.write(staged, { remove: id });
      return undefined;
    }}
    onclose={close}
  />
{:else if open?.kind === 'export'}
  {@const id = open.line.id}
  <WriteDialog
    title="Export {open.line.name ?? id} to GitHub Actions"
    intro="This writes a workflow that runs the schedule on the same cron in CI. Set the schedule to run on GitHub Actions once it is in place, so it does not run twice."
    action="Write workflow"
    onreview={(staged) => list.reviewExport(staged, id)}
    onwrite={async (staged) => {
      const path = await list.writeExport(staged, id);
      return path === undefined ? undefined : `Wrote ${path}.`;
    }}
    onclose={close}
  />
{/if}

<style>
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-bottom: 12px;
  }
  .intro {
    flex: 1 1 320px;
    margin: 0;
  }
  .count {
    font-variant-numeric: tabular-nums;
  }
  .error {
    margin: 0 0 8px;
    color: var(--vc-fail-ink);
  }
</style>
