import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { RunStatus, TestStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { events, fakeEngine } from '#lib/testing/engine.ts';
import RunPanel from './RunPanel.svelte';
import { RunSession } from './session.svelte.ts';

const failingRun = events(
  { case: 'runStarted', value: { files: 1, instances: 2 } },
  {
    case: 'testFinished',
    value: { test: { id: 'a#passes', label: 'Passes', status: TestStatus.PASSED } },
  },
  {
    case: 'testFinished',
    value: {
      test: {
        id: 'a#fails',
        label: 'Fails',
        status: TestStatus.FAILED,
        failure: { message: 'status 500' },
      },
    },
  },
  { case: 'runFinished', value: { status: RunStatus.FAILED, totals: { passed: 1, failed: 1 } } },
  { case: 'report', value: { exit: 1, run: 12n } },
);

function sessionOver(stream: typeof failingRun, seen: boolean[] = []): RunSession {
  return new RunSession(
    fakeEngine({
      async *run(request) {
        seen.push(request.failed);
        for (const event of stream) yield { event };
      },
    }),
  );
}

describe('RunPanel', () => {
  it('starts a run from the empty state and shows its outcome', async () => {
    render(RunPanel, { session: sessionOver(failingRun) });
    await userEvent.click(screen.getByRole('button', { name: 'Run all tests' }));
    expect(await screen.findByRole('status')).toHaveProperty('textContent', 'Failed');
    expect(screen.getByRole('heading', { name: 'Run #12' })).toBeTruthy();
    expect(screen.getByRole('img', { name: /1 passed, 1 failed, .* of 2 tests/ })).toBeTruthy();
    expect(screen.getByRole('log').textContent).toContain('failed: status 500');
  });

  it('re-runs only the failed tests', async () => {
    const seen: boolean[] = [];
    render(RunPanel, { session: sessionOver(failingRun, seen) });
    await userEvent.click(screen.getByRole('button', { name: 'Run all tests' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Re-run failed' }));
    expect(seen).toEqual([false, true]);
  });

  it('filters the log by level and text', async () => {
    render(RunPanel, { session: sessionOver(failingRun) });
    await userEvent.click(screen.getByRole('button', { name: 'Run all tests' }));
    await screen.findByRole('button', { name: 'Re-run failed' });
    const log = screen.getByRole('log');
    await userEvent.click(screen.getByRole('button', { name: 'Errors' }));
    expect(screen.getByRole('button', { name: 'Errors' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(log.textContent).not.toContain('Passes');
    await userEvent.click(screen.getByRole('button', { name: 'All' }));
    await userEvent.type(
      screen.getByRole('searchbox', { name: 'Filter events' }),
      'nothing like it',
    );
    expect(log.textContent).toContain('No events match these filters.');
  });
});
