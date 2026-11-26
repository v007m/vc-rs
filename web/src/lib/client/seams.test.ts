import { describe, expect, it } from 'vitest';
import { RunStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import type { ListRunsRequest } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { engineSeams, historyEntry, noSeams } from './seams.ts';

describe('client seams', () => {
  it('reads a call with no api from its selector, and an api call by its ID', () => {
    const at = { id: 1, outcome: 'passed', startedMs: 5 } as const;
    expect(historyEntry({ ...at, selector: 'GET /users/1' })).toEqual({
      ...at,
      method: 'GET',
      label: '/users/1',
    });
    expect(historyEntry({ ...at, selector: 'users#getUser' })).toEqual({
      ...at,
      label: 'users#getUser',
      api: 'users#getUser',
    });
  });

  it('lists the ad hoc runs as history, newest as the engine gives them', async () => {
    const asked: ListRunsRequest[] = [];
    const seams = engineSeams(
      fakeEngine({
        listRuns: (request) => {
          asked.push(request);
          return {
            runs: [{ id: 7n, selector: 'POST /users', status: RunStatus.FAILED, startedMs: 9n }],
          };
        },
      }),
    );
    const history = await seams.recent(50);
    expect(asked.map((r) => [r.limit, r.kind])).toEqual([[50, 'adhoc']]);
    expect(history).toEqual({
      available: true,
      value: [{ id: 7, method: 'POST', label: '/users', outcome: 'failed', startedMs: 9 }],
    });
  });

  it('says ad hoc calls and streams are not available yet', async () => {
    const seams = engineSeams(fakeEngine({}));
    const adhoc = await seams.callAdhoc({ method: 'GET', url: '/', headers: [], vars: [] });
    const stream = await seams.connect('events#feed', [], () => undefined);
    expect([adhoc.available, stream.available]).toEqual([false, false]);
    expect(await noSeams.recent(5)).toEqual({ available: true, value: [] });
  });
});
