import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { MessageInitShape } from '@bufbuild/protobuf';
import {
  CheckKind,
  RunStatus,
  TestStatus,
  type Run,
  type RunSchema,
  type TestSchema,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { RunSession } from '#lib/run/session.svelte.ts';
import { RunHistory } from './history.svelte.ts';
import { enginePort } from './port.ts';
import RunsScreen from './RunsScreen.svelte';

const MASKED = '••••';

// jsdom has no layout, so `bind:clientHeight` needs a ResizeObserver that never fires.
beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
});

const RUNS: MessageInitShape<typeof RunSchema>[] = [
  {
    id: 7n,
    kind: 'run',
    startedMs: 1_790_000_000_000n,
    finishedMs: 1_790_000_001_500n,
    profiles: ['staging'],
    selector: 'tag:smoke',
    gitBranch: 'main',
    gitCommit: '7d41e0a9c2',
    status: RunStatus.FAILED,
    totals: { passed: 1, failed: 1, blocked: 1 },
  },
  { id: 6n, kind: 'load', startedMs: 1_789_000_000_000n, status: RunStatus.PASSED },
];

const TESTS: MessageInitShape<typeof TestSchema>[] = [
  { id: 'users.crud#listsUsers', label: 'users.crud#listsUsers', status: TestStatus.PASSED },
  {
    id: 'users.crud#createsUser',
    label: 'users.crud#createsUser',
    status: TestStatus.FAILED,
    attempts: 2,
    durationMs: 42,
    failure: { message: 'status was 500, expected 201' },
    checks: [{ kind: CheckKind.STATUS, passed: false, expected: '201', actual: '500' }],
    exchanges: [
      {
        method: 'POST',
        url: 'http://localhost:7421/users',
        status: 500,
        timing: { firstByteMs: 30, downloadMs: 2, totalMs: 40 },
        requestHeaders: [{ name: 'Authorization', value: MASKED }],
      },
    ],
  },
  {
    id: 'users.crud#deletesUser',
    label: 'users.crud#deletesUser',
    status: TestStatus.BLOCKED,
    blockedBy: 'beforeRun seed',
  },
];

function setup() {
  const asked: { lists: number[]; gets: bigint[] } = { lists: [], gets: [] };
  const runs: string[][] = [];
  const engine = fakeEngine({
    listRuns: ({ limit }) => {
      asked.lists.push(limit);
      return { runs: RUNS };
    },
    getRun: ({ id }) => {
      asked.gets.push(id ?? 0n);
      return { run: RUNS[0], tests: TESTS };
    },
    // eslint-disable-next-line require-yield -- the run ends without events
    async *run(request) {
      runs.push(request.selection?.ids ?? []);
    },
  });
  const history = new RunHistory(enginePort(engine));
  return { history, session: new RunSession(engine), asked, runs, user: userEvent.setup() };
}

async function rendered() {
  const s = setup();
  await s.history.load();
  render(RunsScreen, { history: s.history, session: s.session });
  return s;
}

describe('RunsScreen', () => {
  it('lists the recent runs beside the live run', async () => {
    const { asked } = await rendered();
    const recent = screen.getByRole('list', { name: 'Recent runs' });
    expect(within(recent).getByRole('button', { name: 'Run #7, failed' }).textContent).toContain(
      'staging',
    );
    expect(within(recent).getByRole('button', { name: 'Run #6, passed' }).textContent).toContain(
      'load',
    );
    expect(asked.lists).toEqual([20]);
    expect(screen.getByRole('button', { name: 'Run all tests' })).toBeTruthy();
  });

  it('opens a stored run at its first test that did not pass, with its checks and exchanges', async () => {
    const { user, asked } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Run #7, failed' }));
    expect(asked.gets).toEqual([7n]);
    expect(screen.getByRole('heading', { name: 'Run #7' })).toBeTruthy();
    expect(screen.getByText(/Profile staging · Selector tag:smoke/).textContent).toContain(
      'main @ 7d41e0a',
    );
    const detail = screen.getByRole('heading', { name: 'users.crud#createsUser' });
    expect(detail).toBeTruthy();
    expect(screen.getByText('status was 500, expected 201')).toBeTruthy();
    expect(screen.getByText(/2 attempts/)).toBeTruthy();
    expect(within(screen.getByRole('list', { name: 'Checks' })).getByText('status')).toBeTruthy();
    expect(screen.getByText('POST http://localhost:7421/users')).toBeTruthy();
    await user.click(screen.getByRole('option', { name: /deletesUser/ }));
    expect(screen.getByText('Blocked by beforeRun seed')).toBeTruthy();
  });

  it('re-runs the failed and blocked tests of a stored run, then lists the new run', async () => {
    const { user, runs, asked } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Run #7, failed' }));
    await user.click(screen.getByRole('button', { name: 'Re-run failed' }));
    expect(runs).toEqual([['users.crud#createsUser', 'users.crud#deletesUser']]);
    expect(screen.queryByRole('heading', { name: 'Run #7' })).toBeNull();
    await vi.waitFor(() => expect(asked.lists).toEqual([20, 20]));
  });

  it('goes back to the live run', async () => {
    const { user } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Run #7, failed' }));
    await user.click(screen.getByRole('button', { name: 'Back to the current run' }));
    expect(screen.getByRole('button', { name: 'Run all tests' })).toBeTruthy();
  });

  it('says why the history could not be read', async () => {
    const history = new RunHistory({
      runs: () => Promise.reject(new Error('history is locked')),
      run: () => Promise.reject(new Error('unused')),
    });
    await history.load();
    render(RunsScreen, { history, session: new RunSession(fakeEngine({})) });
    expect(screen.getByRole('alert').textContent).toBe('history is locked');
  });
});

describe('enginePort', () => {
  it('reads a stored run with its tests, and refuses one the history does not have', async () => {
    const run: Run | undefined = undefined;
    const port = enginePort(
      fakeEngine({ listRuns: () => ({ runs: RUNS }), getRun: () => ({ run, tests: [] }) }),
    );
    const [newest, load] = await port.runs(20);
    expect(newest).toMatchObject({
      id: 7,
      durationMs: 1500,
      outcome: 'failed',
      counts: { passed: 1, failed: 1, blocked: 1, flaky: 0, skipped: 0 },
      commit: '7d41e0a9c2',
    });
    expect(load).toMatchObject({ id: 6, kind: 'load', outcome: 'passed' });
    expect(load?.durationMs).toBeUndefined();
    await expect(port.run(9)).rejects.toThrow('Run #9 is not in the history.');
  });
});
