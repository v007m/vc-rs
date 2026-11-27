import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { fakeStream } from '#lib/testing/stream.ts';
import type { StreamCall, StreamEvent } from './seams.ts';
import {
  StreamSession,
  frameBody,
  frameKind,
  frameLimit,
  shownFrames,
  type Direction,
  type Schedule,
  type StreamFrame,
} from './stream.svelte.ts';

const call: StreamCall = { api: 'chat#room', vars: [] };
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
/** Flushes in a microtask, so a test that settles sees what came without waiting for a paint. */
const soon: Schedule = (flush) => queueMicrotask(flush);

describe('stream session', () => {
  it('opens, takes messages in order, sends, and closes through the engine', async () => {
    const fake = fakeStream();
    const session = new StreamSession(fake.port, soon);
    const reading = session.connect(call);
    expect(session.status).toBe('connecting');
    fake.push({ kind: 'opened', stream: 's1', runId: 3 });
    fake.push({ kind: 'message', direction: 'in', atMs: 1, json: '{"text":"héllo"}' });
    await settle();
    expect([session.status, session.runId, session.live]).toEqual(['open', 3, true]);
    await session.send('ping');
    await settle();
    expect(session.frames.map((f) => [f.n, f.direction, f.size])).toEqual([
      [1, 'in', 17],
      [2, 'out', 15],
    ]);
    await session.close();
    await reading;
    expect(fake.sent).toEqual([
      ['s1', 'ping'],
      ['s1', undefined],
    ]);
    expect([session.status, session.ended]).toEqual([
      'ended',
      { reason: 'the stream closed', failed: false, code: 1000 },
    ]);
  });

  it('holds messages back while paused and shows them on resume', async () => {
    const fake = fakeStream();
    const session = new StreamSession(fake.port, soon);
    const reading = session.connect(call);
    fake.push({ kind: 'opened', stream: 's1', runId: 0 });
    session.setPaused(true);
    fake.push({ kind: 'message', direction: 'in', atMs: 1, json: '{}' });
    await settle();
    expect([session.frames.length, session.held.length]).toEqual([0, 1]);
    session.setPaused(false);
    expect([session.frames.length, session.held.length]).toEqual([1, 0]);
    session.clear();
    fake.end();
    await reading;
    expect([session.frames.length, session.status]).toEqual([0, 'ended']);
  });

  it('keeps only the newest messages past the limit', async () => {
    const fake = fakeStream();
    const session = new StreamSession(fake.port);
    const reading = session.connect(call);
    const message = (i: number): StreamEvent => ({
      kind: 'message',
      direction: 'in',
      atMs: i,
      json: '1',
    });
    fake.push(...Array.from({ length: frameLimit + 5 }, (_, i) => message(i)));
    fake.end();
    await reading;
    expect(session.frames.length).toBe(frameLimit);
    expect(session.frames[0]?.n).toBe(6);
  });

  it('takes a burst in one flush, keeps the newest in order, and still pauses (D756)', async () => {
    const fake = fakeStream();
    const due: (() => void)[] = [];
    const session = new StreamSession(fake.port, (flush) => due.push(flush));
    const reading = session.connect(call);
    const message = (i: number): StreamEvent => ({
      kind: 'message',
      direction: 'in',
      atMs: i,
      json: `{"I":${i}}`,
    });
    const run = (from: number, count: number) => Array.from({ length: count }, (_, i) => from + i);
    fake.push({ kind: 'opened', stream: 's1', runId: 1 }, ...run(1, 5000).map(message));
    await settle();
    expect([due.length, session.frames.length]).toEqual([1, 0]);
    due.shift()?.();
    expect(session.frames.map((f) => f.n)).toEqual(run(4001, frameLimit));
    expect(session.frames[0]?.search).toBe('{"i":4001}');

    session.setPaused(true);
    fake.push(...run(5001, 5).map(message));
    await settle();
    expect(due.length).toBe(1);
    due.shift()?.();
    expect([session.frames.at(-1)?.n, session.held.map((f) => f.n)]).toEqual([5000, run(5001, 5)]);
    session.setPaused(false);
    expect([session.frames.length, session.frames[0]?.n, session.frames.at(-1)?.n]).toEqual([
      frameLimit,
      4006,
      5005,
    ]);
    expect(session.held).toEqual([]);
    fake.end();
    await reading;
  });

  it('says why a stream failed, but not when it was aborted', async () => {
    const fake = fakeStream();
    const session = new StreamSession(fake.port);
    const failing = session.connect(call);
    fake.end(new ConnectError('no such api', Code.NotFound));
    await failing;
    expect(session.error).toMatch(/no such api/);

    const other = fakeStream();
    const aborted = new StreamSession(other.port);
    const reading = aborted.connect(call);
    await aborted.close();
    await reading;
    expect([aborted.status, aborted.error]).toEqual(['ended', undefined]);
    await aborted.send('late');
    expect(other.sent).toEqual([]);
  });

  it('filters frames by direction and text, and names their kind and body', () => {
    const frame = (n: number, direction: Direction, json: string): StreamFrame => ({
      n,
      direction,
      atMs: 0,
      json,
      size: 1,
      search: json.toLowerCase(),
    });
    const frames = [
      frame(1, 'in', '{"event":"price","data":"{\\"p\\":1}"}'),
      frame(2, 'out', '{"text":"Hello"}'),
      frame(3, 'in', '{"data":"[1,2]"}'),
    ];
    expect(shownFrames(frames, { direction: 'in', filter: '' }).map((f) => f.n)).toEqual([1, 3]);
    expect(shownFrames(frames, { direction: 'all', filter: 'hello' }).map((f) => f.n)).toEqual([2]);
    expect(frames.map((f) => frameKind(f.json))).toEqual(['price', 'text', 'json']);
    expect(frameKind('{"other":1}')).toBe('json');
    expect(frameBody(frames[0]?.json ?? '')).toBe('{\n  "p": 1\n}');
    expect(frameBody(frames[1]?.json ?? '')).toBe('Hello');
    expect(frameBody('{"other":1}')).toBe('{\n  "other": 1\n}');
    const cached = { ...frame(2, 'out', '{}'), search: 'needle' };
    expect(shownFrames([cached], { direction: 'all', filter: 'NEEDLE' })).toEqual([cached]);
  });

  it('unwraps data and text of any JSON type, and falls back to the envelope without them (D754)', () => {
    expect(frameKind('{"data":{"a":1}}')).toBe('json');
    expect(frameBody('{"data":{"a":1}}')).toBe('{\n  "a": 1\n}');
    expect(frameKind('{"event":"tick","data":[1,2]}')).toBe('tick');
    expect(frameBody('{"event":"tick","data":[1,2]}')).toBe('[\n  1,\n  2\n]');
    expect(frameKind('{"data":42}')).toBe('json');
    expect(frameBody('{"data":42}')).toBe('42');
    expect(frameBody('{"data":null,"id":"7"}')).toBe('null');
    expect(frameBody('{"text":true}')).toBe('true');
    expect(frameBody('{"data":"plain words"}')).toBe('plain words');
    expect(frameBody('{"id":"7"}')).toBe('{\n  "id": "7"\n}');
  });
});
