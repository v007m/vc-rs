import type { Row } from './apiForm.ts';
import type { CallOutput } from './port.ts';
import type { AdhocPort, AdhocRequest } from './seams.ts';
import { message } from './tab.svelte.ts';

/** A scratch tab (D539): a request with no api, sent against the project's profiles and auth. */
export class ScratchTab {
  readonly kind = 'scratch';
  readonly id: string;
  title = $state('Untitled request');
  method = $state('GET');
  url = $state('');
  headers: Row[] = $state([]);
  body = $state('');
  sending = $state(false);
  output: CallOutput | undefined = $state();
  /** Why the engine cannot send it yet, when it cannot. */
  unavailable: string | undefined = $state();
  error: string | undefined = $state();
  readonly #port: AdhocPort;

  constructor(port: AdhocPort, id: string) {
    this.#port = port;
    this.id = id;
  }

  get request(): AdhocRequest {
    const headers = this.headers
      .filter((h) => h.on && h.name.trim())
      .map((h) => [h.name.trim(), h.value] as const);
    const sent = { method: this.method, url: this.url.trim(), headers, vars: [] };
    return this.body === '' ? sent : { ...sent, body: this.body };
  }

  addHeader(): void {
    this.headers.push({ name: '', value: '', on: true });
  }

  removeHeader(at: number): void {
    this.headers.splice(at, 1);
  }

  async send(): Promise<void> {
    if (this.sending || this.url.trim() === '') return;
    this.sending = true;
    try {
      const answer = await this.#port.callAdhoc(this.request);
      this.unavailable = answer.available ? undefined : answer.reason;
      this.output = answer.available ? answer.value : undefined;
      this.error = undefined;
    } catch (error) {
      this.error = message(error);
    } finally {
      this.sending = false;
    }
  }
}
