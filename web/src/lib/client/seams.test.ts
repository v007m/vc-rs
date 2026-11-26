import { describe, expect, it } from 'vitest';
import { Code, ConnectError } from '@connectrpc/connect';
import { RunStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import type {
  CallRequest,
  ListRunsRequest,
  StreamSendRequest,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import {
  engineSeams,
  historyEntry,
  noSeams,
  type AdhocRequest,
  type StreamEvent,
} from './seams.ts';

const request: AdhocRequest = {
  type: 'http',
  method: 'POST',
  url: '${baseUrl}/users',
  headers: [['X-Trace', '1']],
  query: [['page', '2']],
  body: '{}',
  auth: 'none',
  vars: ['tenant=acme'],
};

const exchange = {
  method: 'POST',
  url: 'https://api.example.com/users',
  status: 201,
  responseBody: new TextEncoder().encode('{"id":1}'),
};

describe('client seams', () => {
  it('reads a history line from the call summary, or from the selector when there is none', () => {
    const run = { id: 1n, status: RunStatus.PASSED, startedMs: 5n, selector: 'vc-adhoc#call' };
    const at = { id: 1, outcome: 'passed', startedMs: 5 };
    const call = { method: 'GET', url: 'https://api.example.com/users/1', status: 200 };
    expect(historyEntry({ ...run, call })).toEqual({
      ...at,
      method: 'GET',
      label: 'https://api.example.com/users/1',
      status: 200,
    });
    expect(historyEntry(run)).toEqual({ ...at, label: 'vc-adhoc#call' });
  });

  it('lists the ad hoc runs as history, as the engine orders them', async () => {
    const asked: ListRunsRequest[] = [];
    const seams = engineSeams(
      fakeEngine({
        listRuns: (r) => {
          asked.push(r);
          const call = { method: 'POST', url: '/users' };
          return {
            runs: [{ id: 7n, selector: 's', status: RunStatus.FAILED, startedMs: 9n, call }],
          };
        },
      }),
    );
    expect(await seams.recent(50)).toEqual({
      available: true,
      value: [{ id: 7, method: 'POST', label: '/users', outcome: 'failed', startedMs: 9 }],
    });
    expect(asked.map((r) => [r.limit, r.kind])).toEqual([[50, 'adhoc']]);
  });

  it('sends an ad hoc request as the call adhoc field, with no api or draft', async () => {
    const asked: CallRequest[] = [];
    const seams = engineSeams(
      fakeEngine({
        call: (r) => {
          asked.push(r);
          return { exchange };
        },
      }),
    );
    const answer = await seams.callAdhoc(request);
    expect(answer.available && answer.value.exchange?.status).toBe(201);
    const [sent] = asked;
    expect([sent?.api, sent?.draft, sent?.vars]).toEqual(['', undefined, ['tenant=acme']]);
    const { type, method, url, headers, query, body, auth } = sent?.adhoc ?? {};
    expect({ type, method, url, body, auth }).toEqual({
      type: '',
      method: 'POST',
      url: '${baseUrl}/users',
      body: '{}',
      auth: 'none',
    });
    expect([headers, query].map((l) => l?.map((h) => [h.name, h.value]))).toEqual([
      [['X-Trace', '1']],
      [['page', '2']],
    ]);
  });

  it('opens a stream and reads its events in order, then sends and closes on it', async () => {
    const opened: CallRequest[] = [];
    const sent: StreamSendRequest[] = [];
    const seams = engineSeams(
      fakeEngine({
        async *callStream({ call }) {
          if (call) opened.push(call);
          yield { event: { case: 'opened', value: { stream: 's1', runId: 4n, head: exchange } } };
          yield { event: { case: 'message', value: { direction: 'out', atMs: 3n, json: '"hi"' } } };
          yield { event: { case: 'message', value: { direction: 'in', atMs: 8n, json: '{}' } } };
          yield { event: { case: 'ended', value: { reason: 'closed by server', failed: false } } };
        },
        streamSend: (r) => {
          sent.push(r);
          return {};
        },
      }),
    );
    const seen: StreamEvent[] = [];
    const ws = { ...request, type: 'websocket' as const };
    for await (const event of seams.openStream(ws, new AbortController().signal)) seen.push(event);
    expect(opened.map((c) => c.adhoc?.type)).toEqual(['websocket']);
    expect(seen.map((e) => e.kind)).toEqual(['opened', 'message', 'message', 'ended']);
    expect(seen[0]).toMatchObject({ stream: 's1', runId: 4, head: { status: 201 } });
    expect(seen[1]).toEqual({ kind: 'message', direction: 'out', atMs: 3, json: '"hi"' });
    expect(seen[3]).toEqual({ kind: 'ended', reason: 'closed by server', failed: false });
    await seams.streamSend('s1', 'ping');
    await seams.streamSend('s1');
    expect(sent.map((r) => [r.stream, r.text, r.close])).toEqual([
      ['s1', 'ping', false],
      ['s1', undefined, true],
    ]);
  });

  it('opens a stream on an api by its ID with its call vars', async () => {
    const opened: CallRequest[] = [];
    const seams = engineSeams(
      fakeEngine({
        async *callStream({ call }) {
          if (call) opened.push(call);
          yield { event: { case: 'ended', value: { reason: 'done', failed: true, code: 1006 } } };
        },
      }),
    );
    const seen: StreamEvent[] = [];
    const call = { api: 'events#feed', vars: ['room=1'] };
    for await (const event of seams.openStream(call, new AbortController().signal))
      seen.push(event);
    expect(opened.map((c) => [c.api, c.vars, c.adhoc])).toEqual([
      ['events#feed', ['room=1'], undefined],
    ]);
    expect(seen).toEqual([{ kind: 'ended', reason: 'done', failed: true, code: 1006 }]);
  });

  it('says an engine without the RPCs cannot send ad hoc calls or list history yet', async () => {
    const seams = engineSeams(fakeEngine({}));
    const answers = await Promise.all([seams.callAdhoc(request), seams.recent(5)]);
    expect(answers.map((a) => a.available)).toEqual([false, false]);
    const other = engineSeams(
      fakeEngine({
        call: () => {
          throw new ConnectError('no such auth', Code.InvalidArgument);
        },
      }),
    );
    await expect(other.callAdhoc(request)).rejects.toThrow('no such auth');
  });

  it('has no seams by default, and a stream on them fails as unimplemented', async () => {
    expect((await noSeams.callAdhoc(request)).available).toBe(false);
    expect((await noSeams.recent(5)).available).toBe(false);
    const stream = noSeams.openStream(request, new AbortController().signal);
    await expect(stream[Symbol.asyncIterator]().next()).rejects.toThrow(/newer engine/);
  });
});
