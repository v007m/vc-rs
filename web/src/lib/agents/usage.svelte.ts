import { ConnectError } from '@connectrpc/connect';
import type { Usage, UsagePort, UsageWindow } from './usage.ts';

/**
 * The agent usage screen's state: the window picked, what the engine counted in it, and why
 * there is nothing when the engine lacks the RPC (`missing`) or the call failed (`error`).
 */
export class UsageView {
  days: UsageWindow = $state(7);
  usage: Usage | undefined = $state.raw();
  missing: string | undefined = $state();
  error: string | undefined = $state();
  loading = $state(false);
  readonly #port: UsagePort;
  /** The latest load, so an answer for a window no longer picked is dropped. */
  #ask = 0;

  constructor(port: UsagePort) {
    this.#port = port;
  }

  /** Picks `days` and loads its usage. */
  async show(days: UsageWindow): Promise<void> {
    this.days = days;
    await this.load();
  }

  async load(): Promise<void> {
    const ask = ++this.#ask;
    this.loading = true;
    try {
      const got = await this.#port.usage(this.days);
      if (ask !== this.#ask) return;
      this.usage = got.available ? got.value : undefined;
      this.missing = got.available ? undefined : got.reason;
      this.error = undefined;
    } catch (error) {
      if (ask === this.#ask) this.error = ConnectError.from(error).rawMessage;
    } finally {
      if (ask === this.#ask) this.loading = false;
    }
  }
}
