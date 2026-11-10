import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { RunStatus, TestStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { events, fakeEngine } from '#lib/testing/engine.ts';
import { RunSession } from './session.svelte.ts';

const passingRun = events(
  { case: 'runStarted', value: { files: 1, instances: 2 } },
  {
    case: 'testFinished',
    value: {
      test: { id: 'users.crud#listsUsers', label: 'Lists users', status: TestStatus.PASSED },
    },
  },
  {
    case: 'testFinished',
    value: {
      test: {
        id: 'users.crud#createsUser',
        label: 'Creates a user',
        status: TestStatus.FAILED,
        failure: { code: 'V5001', message: 'status 500, expected 201' },
      },
    },
  },
  { case: 'runFinished', value: { status: RunStatus.FAILED, totals: { passed: 1, failed: 1 } } },
  { case: 'report', value: { exit: 1, run: 7n } },
);

describe('RunSession', () => {
  it('builds the view from the stream', async () => {
    const session = new RunSession(
      fakeEngine({
        async *run() {
          for (const event of passingRun) yield { event };
        },
      }),
    );
    await session.start({ profiles: ['dev'] });
    expect(session.view.phase).toBe('failed');
    expect(session.view.planned).toBe(2);
    expect(session.view.counts).toEqual({ passed: 1, failed: 1, blocked: 0, flaky: 0, skipped: 0 });
    expect(session.view.runId).toBe(7n);
    expect(session.view.events.map((e) => [e.level, e.who, e.message])).toEqual([
      ['info', 'run', 'started: 2 tests in 1 files'],
      ['info', 'Lists users', 'passed'],
      ['error', 'Creates a user', 'failed: V5001 status 500, expected 201'],
    ]);
  });

  it('re-runs the last request with only the failed tests', async () => {
    const requests: boolean[] = [];
    const session = new RunSession(
      fakeEngine({
        async *run(request) {
          requests.push(request.failed);
          yield* [];
        },
      }),
    );
    await session.start({ profiles: ['dev'] });
    await session.rerunFailed();
    expect(requests).toEqual([false, true]);
    expect(session.request?.profiles).toEqual(['dev']);
  });

  it('marks a stopped run cancelled', async () => {
    let started!: () => void;
    const begun = new Promise<void>((resolve) => (started = resolve));
    const session = new RunSession(
      fakeEngine({
        async *run(_request, context) {
          yield { event: events({ case: 'runStarted', value: { instances: 3 } })[0] };
          started();
          await new Promise((resolve) => context.signal.addEventListener('abort', resolve));
        },
      }),
    );
    const done = session.start({});
    await begun;
    session.stop();
    await done;
    expect(session.view.phase).toBe('cancelled');
    expect(session.view.planned).toBe(3);
  });

  it('shows why a broken stream ended', async () => {
    const session = new RunSession(
      fakeEngine({
        // eslint-disable-next-line require-yield
        async *run() {
          throw new ConnectError('engine went away', Code.Unavailable);
        },
      }),
    );
    await session.start({});
    expect(session.view.phase).toBe('error');
    expect(session.view.error).toBe('engine went away');
  });
});
