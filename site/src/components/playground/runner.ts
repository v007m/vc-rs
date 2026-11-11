/**
 * The page's side of the runner: it starts the worker on the first Run or Check, sends it the
 * project and passes on what `vc` prints. The page imports this module only when it is needed.
 */
import type { RunMessage, WorkerMessage } from './runner.worker';
import type { Stream } from './wasi';

export type Command = 'run' | 'check';

/** Where the site serves the module the wasm build step writes. */
const MODULE = `${import.meta.env.BASE_URL.replace(/\/$/, '')}/playground/vc-playground.wasm`;

let worker: Worker | undefined;

/**
 * `vc <command>` on `files`: `print` gets each piece of output, with ANSI colours; the promise
 * gives the exit code, or rejects when the runner could not load.
 */
export function play(
  files: Record<string, string>,
  command: Command,
  print: (stream: Stream, text: string) => void,
): Promise<number> {
  const running = (worker ??= new Worker(new URL('./runner.worker.ts', import.meta.url), { type: 'module' }));
  return new Promise((resolve, reject) => {
    running.onmessage = (event: MessageEvent<WorkerMessage>) => {
      const message = event.data;
      if (message.kind === 'out') print(message.stream, message.text);
      else if (message.kind === 'done') resolve(message.exit);
      else reject(new Error(message.message));
    };
    running.onerror = (event) => reject(new Error(event.message || 'the runner stopped'));
    const request = JSON.stringify({ files, command, color: true });
    running.postMessage({ url: MODULE, request } satisfies RunMessage);
  });
}

/** Ends a run that is taking too long; the next run starts a fresh worker. */
export function stop(): void {
  worker?.terminate();
  worker = undefined;
}
