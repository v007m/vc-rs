import { create, type MessageInitShape } from '@bufbuild/protobuf';
import { ConnectError, type Client } from '@connectrpc/connect';
import {
  RunRequestSchema,
  type EngineService,
  type RunRequest,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { apply, broken, idleRun, startingRun, type RunView } from './model.ts';

/** The part of the engine client a run needs, so tests can pass a fake. */
export type RunClient = Pick<Client<typeof EngineService>, 'run'>;

/**
 * The web UI's one live run. Starting a run streams its events into `view`; stopping closes
 * the stream, which the engine treats as one Ctrl-C, so cleanups still run.
 */
export class RunSession {
  view: RunView = $state(idleRun);
  /** The request of the run in `view`, for re-running it. */
  request: RunRequest | undefined = $state();
  #abort: AbortController | undefined;
  readonly #client: RunClient;

  constructor(client: RunClient) {
    this.#client = client;
  }

  get running(): boolean {
    return this.view.phase === 'running';
  }

  /** Starts a run, unless one is running, and resolves when its stream ends. */
  async start(init: MessageInitShape<typeof RunRequestSchema>): Promise<void> {
    if (this.running) return;
    const request = create(RunRequestSchema, init);
    const abort = new AbortController();
    this.#abort = abort;
    this.request = request;
    this.view = startingRun();
    try {
      for await (const { event } of this.#client.run(request, { signal: abort.signal })) {
        if (event) this.view = apply(this.view, event);
      }
    } catch (error) {
      if (!abort.signal.aborted) this.view = broken(this.view, ConnectError.from(error).rawMessage);
    } finally {
      this.#settle(abort);
    }
  }

  /** Re-runs the last request with only the tests that failed in it. */
  rerunFailed(): Promise<void> {
    return this.request ? this.start({ ...this.request, failed: true }) : Promise.resolve();
  }

  /** Stops the running run. */
  stop(): void {
    this.#abort?.abort();
  }

  #settle(abort: AbortController): void {
    if (this.#abort !== abort) return;
    this.#abort = undefined;
    if (this.running)
      this.view = { ...this.view, phase: abort.signal.aborted ? 'cancelled' : 'error' };
  }
}
