import { ConnectError } from '@connectrpc/connect';
import type { LoadPort, LoadRun, LoadUpdate } from './port.ts';

/** The load run on screen: one started here and streaming, or one opened from the history. */
export class LoadSession {
  /** Replaced whole on every update. */
  run: LoadRun | undefined = $state.raw();
  /** Why there is no run: the engine has no load runs, or the stream broke off. */
  note: string | undefined = $state();
  /** The project's load scenarios, to start one; empty until listed. */
  scenarios: readonly string[] = $state([]);
  #abort: AbortController | undefined;
  readonly #port: LoadPort;

  constructor(port: LoadPort) {
    this.#port = port;
  }

  get running(): boolean {
    return this.run?.phase === 'running';
  }

  async open(id: number): Promise<void> {
    await this.#guard(async () => {
      const stored = await this.#port.stored(id);
      if (stored.available) this.run = stored.value;
      else this.note = stored.reason;
    });
  }

  /** Lists the project's load scenarios. */
  async listScenarios(): Promise<void> {
    await this.#guard(async () => {
      const found = await this.#port.scenarios();
      if (found.available) this.scenarios = found.value;
      else this.note = found.reason;
    });
  }

  /** Starts `file` and resolves when its stream ends. */
  async start(file: string): Promise<void> {
    if (this.running) return;
    const abort = new AbortController();
    this.#abort = abort;
    await this.#guard(async () => {
      const started = await this.#port.start(file, abort.signal);
      if (!started.available) {
        this.note = started.reason;
        return;
      }
      this.run = started.value.run;
      for await (const update of started.value.updates) this.run = step(this.run, update);
    });
    if (this.running && this.run) this.run = { ...this.run, phase: 'stopped' };
  }

  /** Stops the running load run by closing its stream, which the engine takes as one Ctrl-C. */
  stop(): void {
    this.#abort?.abort();
  }

  async #guard(work: () => Promise<void>): Promise<void> {
    this.note = undefined;
    try {
      await work();
    } catch (error) {
      if (!this.#abort?.signal.aborted) this.note = ConnectError.from(error).rawMessage;
    }
  }
}

/** `run` after one more update. */
export function step(run: LoadRun, update: LoadUpdate): LoadRun {
  if ('tick' in update) return { ...run, ticks: [...run.ticks, update.tick] };
  return { ...run, id: update.id, phase: update.finished, report: update.report };
}
