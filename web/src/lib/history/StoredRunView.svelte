<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import RunProgress from '#lib/results/RunProgress.svelte';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import Menu from '#lib/ui/Menu.svelte';
  import type { RunHistory } from './history.svelte.ts';
  import { compareHref, exports, lineCommand, span } from './links.ts';
  import { outcomeClass, outcomeLabel } from './outcome.ts';
  import type { StoredRun } from './port.ts';
  import RunDetail from './RunDetail.svelte';
  import type { TestHistoryPort } from './seams.ts';

  /**
   * A run from the history (D509, D636): its header with the Export menu and ⋯ (Re-run failed,
   * Compare with the run before, Copy run command), its facts, then the same view as a live run.
   */
  let {
    stored,
    history,
    session,
    tests,
  }: {
    stored: StoredRun;
    history: RunHistory;
    session: RunSession;
    tests: TestHistoryPort;
  } = $props();

  const run = $derived(stored.run);
  const failed = $derived(history.failedIds);
  const before = $derived(history.previous(run.id));
  let note: string | undefined = $state();

  async function copy(text: string, what: string): Promise<void> {
    await navigator.clipboard.writeText(text);
    note = `Copied ${what}.`;
  }

  async function act(id: string): Promise<void> {
    if (id === 'failed') {
      void session.start({ selection: { ids: failed }, profiles: [...run.profiles] });
      await goto(resolve('/runs/live'));
    }
    if (id === 'compare' && before) await goto(compareHref(before.id, run.id));
    if (id === 'copy') await copy(lineCommand(run), 'the run command');
  }

  async function exported(id: string): Promise<void> {
    const item = exports.find((e) => e.id === id);
    if (item)
      await copy(
        item.command(run.id),
        item.label === 'HTML report path' ? 'the path' : 'the command',
      );
  }
</script>

<header class="head">
  <a class="back st-secondary" href={resolve('/runs')}>‹ All runs</a>
  <div class="title">
    <h2 class="st-title2">Run #{run.id}</h2>
    <span class="st-badge {outcomeClass[run.outcome]}">{outcomeLabel[run.outcome]}</span>
    <span class="vc-grow"></span>
    <Menu
      label="Export run #{run.id}"
      items={exports.map((e) => ({ id: e.id, label: e.label, meta: 'Copy' }))}
      onpick={(id) => void exported(id)}>Export</Menu
    >
    <Menu
      label="More for run #{run.id}"
      button="st-icon-btn"
      items={[
        {
          id: 'failed',
          label: failed.length ? `Re-run ${failed.length} failed` : 'Re-run failed',
          disabled: !failed.length || session.running,
        },
        {
          id: 'compare',
          label: before ? `Compare with #${before.id}` : 'Compare with previous',
          disabled: !before,
        },
        { id: 'copy', label: 'Copy run command' },
      ]}
      onpick={(id) => void act(id)}><span aria-hidden="true">⋯</span></Menu
    >
  </div>
  <dl class="facts">
    <div>
      <dt>Selector</dt>
      <dd class="vc-mono">{run.selector || 'all tests'}</dd>
    </div>
    <div>
      <dt>Profile</dt>
      <dd>{run.profiles.join(', ') || 'default'}</dd>
    </div>
    {#if run.branch || run.commit}
      <div>
        <dt>Branch</dt>
        <dd class="vc-mono">{[run.branch, run.commit?.slice(0, 7)].filter(Boolean).join(' @ ')}</dd>
      </div>
    {/if}
    <div>
      <dt>Started</dt>
      <dd>
        <time datetime={new Date(run.startedMs).toISOString()}
          >{new Date(run.startedMs).toLocaleString()}</time
        >
      </dd>
    </div>
    <div>
      <dt>Duration</dt>
      <dd>{span(run.durationMs)}</dd>
    </div>
  </dl>
  {#if note}<p class="vc-sr" role="status">{note}</p>{/if}
</header>
<RunProgress counts={run.counts} planned={0} />
{#if stored.tests.length}
  <RunDetail
    label="Tests in run #{run.id}"
    tests={stored.tests}
    bind:current={history.current}
    history={tests}
  />
{:else}
  <p class="st-secondary">This run kept no test results.</p>
{/if}

<style>
  .head {
    display: grid;
    gap: 8px;
  }
  .back {
    justify-self: start;
    text-decoration: none;
  }
  .back:hover {
    text-decoration: underline;
  }
  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  h2 {
    margin: 0;
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 24px;
    margin: 0;
  }
  dt {
    font: var(--st-type-caption);
    color: var(--st-text-tertiary);
  }
  dd {
    margin: 2px 0 0;
    font-variant-numeric: tabular-nums;
  }
</style>
