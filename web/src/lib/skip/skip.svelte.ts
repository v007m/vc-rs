import { ConnectError } from '@connectrpc/connect';
import type { ChangesOutput } from '#lib/client/port.ts';
import type { SkipPort } from './port.ts';

/**
 * One skip change, previewed then written as `vc skip` or `vc unskip` writes it: a skip of
 * `target` with a reason and an optional last day, or the removal of its skip.
 */
export class SkipChange {
  reason = $state('');
  /** `YYYY-MM-DD`, or empty for no end date. */
  until = $state('');
  preview: ChangesOutput | undefined = $state();
  /** What the write did, once written. */
  written: readonly string[] | undefined = $state();
  busy = $state(false);
  error: string | undefined = $state();
  readonly target: string;
  /** Removes the skip rather than writing one. */
  readonly removing: boolean;
  readonly #port: SkipPort;

  constructor(port: SkipPort, target: string, removing = false) {
    this.#port = port;
    this.target = target;
    this.removing = removing;
  }

  /** A skip needs a reason; a removal needs nothing. */
  get ready(): boolean {
    return !this.busy && (this.removing || this.reason.trim() !== '');
  }

  /** Changes the reason or the date; the preview no longer matches, so it is dropped. */
  set(field: 'reason' | 'until', value: string): void {
    this[field] = value;
    this.preview = undefined;
  }

  async showPreview(): Promise<void> {
    if (!this.ready) return;
    const changes = await this.#send(false);
    if (changes) this.preview = changes;
  }

  /** Writes the change the preview showed. */
  async write(): Promise<void> {
    if (!this.preview || !this.ready) return;
    const changes = await this.#send(true);
    if (changes) this.written = changes.titles;
  }

  async #send(write: boolean): Promise<ChangesOutput | undefined> {
    this.busy = true;
    try {
      const changes = this.removing
        ? await this.#port.unskip(this.target, write)
        : await this.#port.skip(this.#input(), write);
      this.error = undefined;
      return changes;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
      return undefined;
    } finally {
      this.busy = false;
    }
  }

  #input() {
    const reason = this.reason.trim();
    return this.until
      ? { target: this.target, reason, until: this.until }
      : { target: this.target, reason };
  }
}
