import { Code, ConnectError } from '@connectrpc/connect';
import type { StreamCall, StreamEvent, StreamPort } from '#lib/client/seams.ts';

/** A stream port whose stream yields what the test pushes, until the test ends it. */
export function fakeStream() {
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
