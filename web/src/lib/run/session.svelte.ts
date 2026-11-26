import { create, type MessageInitShape } from '@bufbuild/protobuf';
import { Code, ConnectError, type Client } from '@connectrpc/connect';
import {
  CancelLevel,
  RunRequestSchema,
  type EngineService,
  type RunRequest,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { apply, broken, idleRun, startingRun, type RunView } from './model.ts';

/** The part of the engine client a run needs, so tests can pass a fake. */
export type RunClient = Pick<Client<typeof EngineService>, 'run' | 'cancel'>;

/** How far a stop has gone: `none`, `stopping` (cleanups run) or `abandoned` (they do not). */
export type StopState = 'none' | 'stopping' | 'abandoned';

/**
 * The web UI's one live run. Each run is named by a fresh handle (D551), so it can be cancelled
 * from the first moment: **Stop** is one Ctrl-C, so work in flight gets its grace period and the
 * cleanups run, and a second Stop ends the grace period; **Skip cleanups** abandons them (D552).
 * The stream stays open through a stop and says how the run ended. An engine without Cancel is
 * stopped by closing the stream, which it treats as one Ctrl-C.
 */
export class RunSession {
  /** Replaced whole on every event, never changed in place, so it is not made deeply reactive. */
  view: RunView = $state.raw(idleRun);
  /** The request of the run in `view`, for re-running it. */
  request: RunRequest | undefined = $state();
  stopState: StopState = $state('none');
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
    const request = create(RunRequestSchema, { ...init, handle: crypto.randomUUID() });
    const abort = new AbortController();
    this.#abort = abort;
    this.request = request;
    this.stopState = 'none';
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

  /** Stops the running run; a second call ends the grace period of the work in flight. */
  stop(): Promise<void> {
    if (this.stopState === 'none') this.stopState = 'stopping';
    return this.#cancel(CancelLevel.STOP);
  }

  /** Stops the running run at once and abandons the cleanups still to run. */
  skipCleanups(): Promise<void> {
    this.stopState = 'abandoned';
    return this.#cancel(CancelLevel.ABANDON);
  }

  /** Sends `level`; a run that already returned (`not_found`) has nothing left to cancel. */
  async #cancel(level: CancelLevel): Promise<void> {
    const abort = this.#abort;
    const run = this.request?.handle;
    if (!abort || !run) return;
    try {
      await this.#client.cancel({ run, level });
    } catch (error) {
      const { code } = ConnectError.from(error);
      if (code !== Code.NotFound) abort.abort();
    }
  }

  #settle(abort: AbortController): void {
    if (this.#abort !== abort) return;
    this.#abort = undefined;
    const stopped = abort.signal.aborted || this.stopState !== 'none';
    if (this.running) this.view = { ...this.view, phase: stopped ? 'cancelled' : 'error' };
  }
}
