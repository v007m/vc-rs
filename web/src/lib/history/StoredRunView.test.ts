import type { MessageInitShape } from '@bufbuild/protobuf';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { afterEach, beforeAll, expect, it, vi } from 'vitest';
import { goto } from '$app/navigation';
import {
  RunStatus,
  TestStatus,
  type RunSchema,
  type TestSchema,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { RunSession } from '#lib/run/session.svelte.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { stubResizeObserver } from '#lib/testing/layout.ts';
import { RunHistory } from './history.svelte.ts';
import { enginePort } from './port.ts';
import { noRunsSeams } from './seams.ts';
import StoredRunView from './StoredRunView.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

beforeAll(stubResizeObserver);
afterEach(() => vi.mocked(goto).mockClear());

const RUNS: MessageInitShape<typeof RunSchema>[] = [
  {
    id: 7n,
    kind: 'run',
    startedMs: 1_790_000_000_000n,
    finishedMs: 1_790_000_001_500n,
    profiles: ['staging'],
    selector: 'tag:smoke',
    status: RunStatus.FAILED,
    totals: { passed: 1, failed: 1, blocked: 1 },
  },
  { id: 6n, kind: 'run', startedMs: 1_789_000_000_000n, status: RunStatus.PASSED },
];

const TESTS: MessageInitShape<typeof TestSchema>[] = [
  { id: 'users.crud#listsUsers', label: 'users.crud#listsUsers', status: TestStatus.PASSED },
  {
    id: 'users.crud#createsUser',
    label: 'users.crud#createsUser',
    status: TestStatus.FAILED,
    failure: { message: 'status was 500, expected 201' },
  },
  { id: 'users.crud#deletesUser', label: 'users.crud#deletesUser', status: TestStatus.BLOCKED },
];

async function rendered() {
  const started: string[][] = [];
  const engine = fakeEngine({
    listRuns: () => ({ runs: RUNS }),
    getRun: () => ({ run: RUNS[0], tests: TESTS }),
    async *run(request) {
      started.push(request.selection?.ids ?? []);
      yield* [];
    },
  });
  const history = new RunHistory(enginePort(engine));
  await history.load();
  await history.show(7);
  const user = userEvent.setup();
  const stored = history.open;
  if (!stored) throw new Error('run #7 did not open');
  const session = new RunSession(engine);
  render(StoredRunView, { stored, history, session, tests: noRunsSeams.tests });
  return { started, user };
}

it('heads the run with its number, outcome and first test that did not pass', async () => {
  await rendered();
  expect(screen.getByRole('heading', { level: 2, name: 'Run #7' })).toBeTruthy();
  expect(screen.getByText('Failed')).toBeTruthy();
  expect(screen.getByText('status was 500, expected 201')).toBeTruthy();
});

it('copies the command that writes the run’s HTML report', async () => {
  const { user } = await rendered();
  await user.click(screen.getByRole('button', { name: 'Export run #7' }));
  await user.click(screen.getByRole('menuitem', { name: 'HTML report Copy' }));
  expect(await navigator.clipboard.readText()).toBe('vc report 7 --format html');
  expect(screen.getByRole('status').textContent).toBe('Copied the command.');
});

it('re-runs the failed and blocked tests, then goes to the live run', async () => {
  const { started, user } = await rendered();
  await user.click(screen.getByRole('button', { name: 'More for run #7' }));
  await user.click(screen.getByRole('menuitem', { name: 'Re-run 2 failed' }));
  await vi.waitFor(() =>
    expect(started).toEqual([['users.crud#createsUser', 'users.crud#deletesUser']]),
  );
  expect(goto).toHaveBeenCalledWith('/runs/live');
});

it('compares the run with the one before it', async () => {
  const { user } = await rendered();
  await user.click(screen.getByRole('button', { name: 'More for run #7' }));
  await user.click(screen.getByRole('menuitem', { name: 'Compare with #6' }));
  expect(goto).toHaveBeenCalledWith('/runs/compare/6/7');
});
