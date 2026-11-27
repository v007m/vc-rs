import { describe, expect, it } from 'vitest';
import type { RunView } from '#lib/run/model.ts';
import { runEnded } from './runToast.ts';

const counts = { passed: 38, failed: 1, blocked: 1, flaky: 0, skipped: 0 };
const view = (over: Partial<RunView>): RunView =>
  ({ phase: 'passed', planned: 40, counts, ...over }) as RunView;

describe('runEnded', () => {
  it('names the run and its tests when it passes or fails', () => {
    expect(
      runEnded(view({ runId: 12n, counts: { ...counts, failed: 0, blocked: 0, passed: 40 } })),
    ).toEqual({ text: 'Run #12 passed: 40 tests.', tone: 'done', id: 12 });
    expect(runEnded(view({ phase: 'failed', runId: 13n })).text).toBe(
      'Run #13 failed: 1 failed, 1 blocked of 40 tests.',
    );
  });

  it('reads a stopped or broken run, with no number before the history has one', () => {
    expect(runEnded(view({ phase: 'cancelled' }))).toEqual({
      text: 'The run was stopped after 40 tests.',
      tone: 'done',
    });
    expect(runEnded(view({ phase: 'error', error: 'connection lost' }))).toEqual({
      text: 'The run broke off: connection lost.',
      tone: 'error',
    });
  });
});
