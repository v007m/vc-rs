import { Code, ConnectError } from '@connectrpc/connect';
import type { PreviewedChanges, SkipPort } from './port.ts';

/**
 * One skip change, previewed then written as `vc skip` or `vc unskip` writes it: a skip of
 * `target` with a reason and an optional last day, or the removal of its skip.
 */
export class SkipChange {
  reason = $state('');
  /** `YYYY-MM-DD`, or empty for no end date. */
  until = $state('');
  preview: PreviewedChanges | undefined = $state();
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
    const changes = await this.#send();
    if (changes) this.preview = changes;
  }

  /**
   * Writes the change the preview showed, only while its file is the one previewed; when the
   * file changed since, the preview is dropped, to be shown again.
   */
  async write(): Promise<void> {
    if (!this.preview || !this.ready) return;
    const changes = await this.#send(this.preview.digest);
    if (changes) this.written = changes.titles;
  }

  async #send(digest?: string): Promise<PreviewedChanges | undefined> {
    this.busy = true;
    try {
      const changes = this.removing
        ? await this.#port.unskip(this.target, digest)
        : await this.#port.skip(this.#input(), digest);
      this.error = undefined;
      return changes;
    } catch (error) {
      const refused = ConnectError.from(error);
      if (refused.code === Code.Aborted) this.preview = undefined;
      this.error = refused.rawMessage;
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
