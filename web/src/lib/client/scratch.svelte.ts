import type { Row } from './apiForm.ts';
import type { CallOutput } from './port.ts';
import {
  adhocWords,
  type AdhocPort,
  type AdhocRequest,
  type CallType,
  type Pair,
  type StreamPort,
} from './seams.ts';
import { StreamSession } from './stream.svelte.ts';
import { message } from './tab.svelte.ts';

function pairs(rows: readonly Row[]): Pair[] {
  return rows.filter((r) => r.on && r.name.trim()).map((r) => [r.name.trim(), r.value] as const);
}

/** `output` with its diagnostics naming "this request" in place of the stand-in file. */
function plain(output: CallOutput): CallOutput {
  const notices = output.notices.map(adhocWords);
  return { ...output, text: adhocWords(output.text), notices };
}

/**
 * A scratch tab (D539): a request with no api, sent against the project's profiles and auth.
 * An SSE or WebSocket request opens a stream in place of a send (D564).
 */
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
  readonly stream: StreamSession;
  readonly #port: AdhocPort;

  constructor(port: AdhocPort & StreamPort, id: string) {
    this.#port = port;
    this.id = id;
    this.stream = new StreamSession(port);
  }

  /** What the method pill shows: the HTTP method, or `SSE` or `WS` for a stream. */
  get badge(): string {
    if (this.type === 'http') return this.method;
    return this.type === 'sse' ? 'SSE' : 'WS';
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

  /** Sends an HTTP request; an SSE or WebSocket one opens its stream, which reads on its own. */
  async send(): Promise<void> {
    if (this.sending || this.url.trim() === '') return;
    if (this.type !== 'http') {
      void this.stream.connect(this.request);
      return;
    }
    this.sending = true;
    try {
      const answer = await this.#port.callAdhoc(this.request);
      this.unavailable = answer.available ? undefined : answer.reason;
      this.output = answer.available ? plain(answer.value) : undefined;
      this.error = undefined;
    } catch (error) {
      this.error = adhocWords(message(error));
    } finally {
      this.sending = false;
    }
  }
}
