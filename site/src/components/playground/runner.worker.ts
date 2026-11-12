/// <reference lib="webworker" />
/**
 * Runs `vc-playground.wasm` off the page's thread: each message is one `vc check` or `vc run`
 * of the files it carries, and the worker posts the text the program writes as it writes it.
 */
import { Wasi, type Stream } from './wasi';

export interface RunMessage {
  /** Where the page serves the module. */
  url: string;
  /** The request the program reads on stdin, as JSON. */
  request: string;
}

export type WorkerMessage =
  | { kind: 'out'; stream: Stream; text: string }
  | { kind: 'done'; exit: number }
  | { kind: 'failed'; message: string };

const scope = self as unknown as DedicatedWorkerGlobalScope;
const modules = new Map<string, Promise<WebAssembly.Module>>();

function compiled(url: string): Promise<WebAssembly.Module> {
  let module = modules.get(url);
  if (!module) {
    module = fetch(url).then((response) => {
      if (!response.ok) throw new Error(`the runner is not on this copy of the site (${response.status} for ${url})`);
      return WebAssembly.compileStreaming(response);
    });
    module.catch(() => modules.delete(url));
    modules.set(url, module);
  }
  return module;
}

async function run({ url, request }: RunMessage): Promise<WorkerMessage> {
  const module = await compiled(url);
  const post = (stream: Stream, text: string) =>
    scope.postMessage({ kind: 'out', stream, text } satisfies WorkerMessage);
  const wasi = new Wasi({ stdin: new TextEncoder().encode(request), write: post });
  const instance = await WebAssembly.instantiate(module, wasi.imports(module));
  return { kind: 'done', exit: wasi.start(instance) };
}

scope.onmessage = (event: MessageEvent<RunMessage>) => {
  run(event.data)
    .catch(
      (error: unknown): WorkerMessage => ({
        kind: 'failed',
        message: error instanceof Error ? error.message : String(error),
      }),
    )
    .then((message) => scope.postMessage(message));
};
