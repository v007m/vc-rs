import { ConnectError } from '@connectrpc/connect';
import { maxMessageBytes } from '#lib/api/limits.ts';
import type { ChangesOutput, ClientPort } from './port.ts';

/** The largest HAR file the dialog reads: one message, less room for the request around it. */
export const maxHarBytes = maxMessageBytes - 64 * 1024;

const maxHarSize = `${(maxHarBytes / 1024 / 1024).toFixed(1)} MB`;

/**
 * The HAR import dialog's state: pick a recording, preview what the import would write, then
 * write it. The engine filters, maps and strips secrets the same way `vc import har` does.
 */
export class HarImport {
  fileName: string | undefined = $state();
  flow = $state('');
  preview: ChangesOutput | undefined = $state();
  busy = $state(false);
  written: readonly string[] | undefined = $state();
  error: string | undefined = $state();
  #source: string | undefined;
  readonly #port: Pick<ClientPort, 'importHar'>;

  constructor(port: Pick<ClientPort, 'importHar'>) {
    this.#port = port;
  }

  /** Reads the recording `file`; a new file drops the last preview. */
  async choose(file: File): Promise<void> {
    this.preview = undefined;
    this.written = undefined;
    if (file.size > maxHarBytes) {
      this.fileName = undefined;
      this.#source = undefined;
      this.error = `${file.name} is too large (max ${maxHarSize}).`;
      return;
    }
    this.fileName = file.name;
    this.#source = await file.text();
    this.error = undefined;
  }

  /** Changes the flow test ID; the preview no longer matches, so it is dropped. */
  setFlow(flow: string): void {
    this.flow = flow;
    this.preview = undefined;
  }

  get ready(): boolean {
    return this.#source !== undefined && !this.busy;
  }

  async showPreview(): Promise<void> {
    const changes = await this.#import(false);
    if (changes) this.preview = changes;
  }

  /** Writes the import the preview showed. */
  async write(): Promise<void> {
    if (!this.preview) return;
    const changes = await this.#import(true);
    if (changes) this.written = changes.titles;
  }

  async #import(write: boolean): Promise<ChangesOutput | undefined> {
    const source = this.#source;
    if (source === undefined || this.busy) return undefined;
    this.busy = true;
    try {
      const flow = this.flow.trim() || undefined;
      const changes = await this.#port.importHar({ source, flow, write });
      this.error = undefined;
      return changes;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
      return undefined;
    } finally {
      this.busy = false;
    }
  }
}
