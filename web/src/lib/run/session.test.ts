import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { CancelLevel } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
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

  /** A run that starts, then waits until `release` ends it or its stream closes. */
  function heldRun(cancels: { run: string; level: CancelLevel }[], answer?: Code) {
    let started!: () => void;
    let release!: () => void;
    const begun = new Promise<void>((resolve) => (started = resolve));
    const released = new Promise<void>((resolve) => (release = resolve));
    const handles: string[] = [];
    const client = fakeEngine({
      async *run(request, context) {
        handles.push(request.handle);
        yield { event: events({ case: 'runStarted', value: { instances: 3 } })[0] };
        started();
        await Promise.race([
          released,
          new Promise((resolve) => context.signal.addEventListener('abort', resolve)),
        ]);
      },
      cancel({ run, level }) {
        cancels.push({ run, level });
        if (answer !== undefined) throw new ConnectError('no such run', answer);
        if (level === CancelLevel.ABANDON) release();
        return {};
      },
    });
    return { session: new RunSession(client), begun, handles, release };
  }

  it('names each run by a fresh handle and stops it by that handle, twice to end the grace', async () => {
    const cancels: { run: string; level: CancelLevel }[] = [];
    const { session, begun, handles, release } = heldRun(cancels);
    const done = session.start({});
    await begun;
    await session.stop();
    expect(session.stopState).toBe('stopping');
    expect(session.running).toBe(true);
    await session.stop();
    release();
    await done;
    expect(handles[0]).toMatch(/^[0-9a-f-]{36}$/);
    expect(cancels).toEqual([
      { run: handles[0], level: CancelLevel.STOP },
      { run: handles[0], level: CancelLevel.STOP },
    ]);
    expect(session.view.phase).toBe('cancelled');
    expect(session.view.planned).toBe(3);
  });

  it('skips the cleanups with abandon', async () => {
    const cancels: { run: string; level: CancelLevel }[] = [];
    const { session, begun } = heldRun(cancels);
    const done = session.start({});
    await begun;
    await session.skipCleanups();
    await done;
    expect(cancels.map((c) => c.level)).toEqual([CancelLevel.ABANDON]);
    expect(session.stopState).toBe('abandoned');
    expect(session.view.phase).toBe('cancelled');
  });

  it('takes not_found as a run that already finished, and leaves its stream alone', async () => {
    const { session, begun, release } = heldRun([], Code.NotFound);
    const done = session.start({});
    await begun;
    await session.stop();
    expect(session.running).toBe(true);
    release();
    await done;
    expect(session.view.phase).toBe('cancelled');
  });

  it('closes the stream when the engine has no Cancel', async () => {
    const { session, begun } = heldRun([], Code.Unimplemented);
    const done = session.start({});
    await begun;
    await session.stop();
    await done;
    expect(session.view.phase).toBe('cancelled');
  });

  it('starts the next run unstopped, with a new handle', async () => {
    const { session, begun, handles, release } = heldRun([]);
    const first = session.start({});
    await begun;
    await session.stop();
    release();
    await first;
    await session.start({});
    expect(session.stopState).toBe('none');
    expect(new Set(handles).size).toBe(2);
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

  it('shows a refused reports.dir as the engine words it', async () => {
    const said = 'V5132: reports.dir must be a folder inside the project';
    const session = new RunSession(
      fakeEngine({
        // eslint-disable-next-line require-yield
        async *run() {
          throw new ConnectError(said, Code.InvalidArgument);
        },
      }),
    );
    await session.start({});
    expect(session.view.error).toBe(said);
  });
});
