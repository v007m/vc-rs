import type { Row } from './apiForm.ts';
import type { CallOutput } from './port.ts';
import type { AdhocPort, AdhocRequest, CallType, Pair } from './seams.ts';
import { message } from './tab.svelte.ts';

function pairs(rows: readonly Row[]): Pair[] {
  return rows.filter((r) => r.on && r.name.trim()).map((r) => [r.name.trim(), r.value] as const);
}

/** A scratch tab (D539): a request with no api, sent against the project's profiles and auth. */
export class ScratchTab {
  readonly kind = 'scratch';
  readonly id: string;
  title = $state('Untitled request');
  type: CallType = $state('http');
  method = $state('GET');
  url = $state('');
  headers: Row[] = $state([]);
  query: Row[] = $state([]);
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
    const lists = { headers: pairs(this.headers), query: pairs(this.query) };
    const sent = { type: this.type, method: this.method, url: this.url.trim(), ...lists, vars: [] };
    return this.body === '' ? sent : { ...sent, body: this.body };
  }

  addRow(list: 'headers' | 'query'): void {
    this[list].push({ name: '', value: '', on: true });
  }

  removeRow(list: 'headers' | 'query', at: number): void {
    this[list].splice(at, 1);
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
