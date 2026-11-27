import { Code, ConnectError } from '@connectrpc/connect';
import type { ApiInfo, CallInput, CallOutput, ClientPort } from './port.ts';
import { ApiRequest } from './request.svelte.ts';
import { parseResponse, scalarFields, varArg, type CallResponse } from './response.ts';

export type VarRow = { name: string; value: string };

/** One call and what came back: the input as sent, the engine's output and the parsed response. */
export type CallResult = Readonly<{
  input: CallInput;
  output: CallOutput;
  response: CallResponse | undefined;
  fields: readonly string[];
}>;

export function message(error: unknown): string {
  return ConnectError.from(error).rawMessage;
}

/** An open Client tab on one api (D539): its request draft, call vars and last result. */
export class CallTab {
  readonly kind = 'api';
  readonly id: string;
  readonly api: ApiInfo;
  readonly request: ApiRequest;
  vars: VarRow[] = $state([]);
  keep = $state(false);
  sending = $state(false);
  result: CallResult | undefined = $state();
  error: string | undefined = $state();
  readonly #port: ClientPort;

  constructor(port: ClientPort, id: string, api: ApiInfo) {
    this.#port = port;
    this.id = id;
    this.api = api;
    this.vars = api.vars.map((name) => ({ name, value: '' }));
    this.request = new ApiRequest(port, api.id);
  }

  get title(): string {
    return this.api.name;
  }

  async load(): Promise<void> {
    await this.request.load();
  }

  addVar(): void {
    this.vars.push({ name: '', value: '' });
  }

  removeVar(index: number): void {
    this.vars.splice(index, 1);
  }

  get input(): CallInput {
    const vars = this.vars
      .filter((v) => v.name.trim() && v.value !== '')
      .map((v) => varArg(v.name.trim(), v.value));
    const draft = this.request.draft;
    const input = { api: this.api.id, vars, keep: this.keep };
    return draft ? { ...input, draft } : input;
  }

  async explain(): Promise<boolean> {
    try {
      await this.request.explain(this.input.vars);
      this.error = undefined;
      return true;
    } catch (error) {
      this.#failed(error);
      return false;
    }
  }

  /** Sends the request; `keep` once skips the api's cleanup for this call alone. */
  async send(keep = this.keep): Promise<void> {
    if (this.sending || this.request.problem !== undefined) return;
    const input = { ...this.input, keep };
    this.sending = true;
    try {
      const output = await this.#port.call(input);
      const response = parseResponse(output.responseJson);
      const fields = response ? scalarFields(response.body) : [];
      this.result = { input, output, response, fields };
      this.error = undefined;
    } catch (error) {
      this.#failed(error);
    } finally {
      this.sending = false;
    }
  }

  async saveAsTest(testId: string, fields: readonly string[]): Promise<string | undefined> {
    if (!this.result) return undefined;
    try {
      const { input, output } = this.result;
      const changes = await this.#port.saveCallAsTest({
        testId,
        api: input.api,
        vars: input.vars,
        responseJson: output.responseJson,
        fields,
      });
      this.error = undefined;
      return changes.titles.join(' · ') || `Added ${testId}`;
    } catch (error) {
      this.error = message(error);
      return undefined;
    }
  }

  #failed(error: unknown): void {
    if (ConnectError.from(error).code === Code.Aborted) this.request.stale();
    this.error = message(error);
  }
}
