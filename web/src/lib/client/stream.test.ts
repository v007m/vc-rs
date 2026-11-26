import { describe, expect, it } from 'vitest';
import { Code, ConnectError } from '@connectrpc/connect';
import type { StreamCall, StreamEvent, StreamPort } from './seams.ts';
import {
  StreamSession,
  frameBody,
  frameKind,
  frameLimit,
  shownFrames,
  type StreamFrame,
} from './stream.svelte.ts';

/** A stream port whose stream yields what the test pushes, until the test ends it. */
function fakeStream() {
  const queue: StreamEvent[] = [];
  const sent: [string, string | undefined][] = [];
  let wake: (() => void) | undefined;
  let done = false;
  let fail: Error | undefined;
  const poke = () => wake?.();
  const port: StreamPort = {
    async *openStream(_call: StreamCall, signal: AbortSignal) {
      signal.addEventListener('abort', () => {
        fail = new ConnectError('aborted', Code.Canceled);
        poke();
      });
      for (;;) {
        const next = queue.shift();
        if (next) yield next;
        else if (fail) throw fail;
        else if (done) return;
        else await new Promise<void>((resolve) => (wake = resolve));
      }
    },
    streamSend(stream, text) {
      sent.push([stream, text]);
      if (text === undefined) {
        queue.push({ kind: 'ended', reason: 'the stream closed', failed: false, code: 1000 });
        done = true;
      } else
        queue.push({ kind: 'message', direction: 'out', atMs: 2, json: '{"text":"' + text + '"}' });
      poke();
      return Promise.resolve();
    },
  };
  const push = (...events: StreamEvent[]) => {
    queue.push(...events);
    poke();
  };
  const end = (error?: Error) => {
    fail = error;
    done = true;
    poke();
  };
  return { port, sent, push, end };
}

const call: StreamCall = { api: 'chat#room', vars: [] };
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('stream session', () => {
  it('opens, takes messages in order, sends, and closes through the engine', async () => {
    const fake = fakeStream();
    const session = new StreamSession(fake.port);
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
    const session = new StreamSession(fake.port);
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
    const frames: StreamFrame[] = [
      { n: 1, direction: 'in', atMs: 0, json: '{"event":"price","data":"{\\"p\\":1}"}', size: 1 },
      { n: 2, direction: 'out', atMs: 0, json: '{"text":"Hello"}', size: 1 },
      { n: 3, direction: 'in', atMs: 0, json: '{"data":"[1,2]"}', size: 1 },
    ];
    expect(shownFrames(frames, { direction: 'in', filter: '' }).map((f) => f.n)).toEqual([1, 3]);
    expect(shownFrames(frames, { direction: 'all', filter: 'hello' }).map((f) => f.n)).toEqual([2]);
    expect(frames.map((f) => frameKind(f.json))).toEqual(['price', 'text', 'json']);
    expect(frameKind('{"other":1}')).toBe('json');
    expect(frameBody(frames[0]?.json ?? '')).toBe('{\n  "p": 1\n}');
    expect(frameBody(frames[1]?.json ?? '')).toBe('Hello');
    expect(frameBody('{"other":1}')).toBe('{\n  "other": 1\n}');
  });
});
