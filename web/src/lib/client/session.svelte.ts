import { Code, ConnectError } from '@connectrpc/connect';
import type { ApiInfo, CallInput, CallOutput, ClientPort, FlowInfo } from './port.ts';
import { ApiRequest } from './request.svelte.ts';
import { parseResponse, scalarFields, varArg, type CallResponse } from './response.ts';

/** A var row in the request: a name the api declares, or one the user added. */
export type VarRow = { name: string; value: string };

/** The last call's result, with its response read. */
export type CallResult = Readonly<{
  input: CallInput;
  output: CallOutput;
  response: CallResponse | undefined;
  /** The body fields a test saved from this call can expect. */
  fields: readonly string[];
}>;

function message(error: unknown): string {
  return ConnectError.from(error).rawMessage;
}

/**
 * The API client's state: the project's apis, the open one's vars and request, and the last
 * call. A call goes through the engine like `vc call`, so it uses the project's auth and active
 * profile, with the api as the request edited it.
 */
export class ClientSession {
  apis: readonly ApiInfo[] = $state([]);
  flows: readonly FlowInfo[] = $state([]);
  current: ApiInfo | undefined = $state();
  request: ApiRequest | undefined = $state();
  vars: VarRow[] = $state([]);
  keep = $state(false);
  sending = $state(false);
  result: CallResult | undefined = $state();
  error: string | undefined = $state();
  readonly #port: ClientPort;

  constructor(port: ClientPort) {
    this.#port = port;
  }

  async load(): Promise<void> {
    try {
      [this.apis, this.flows] = await Promise.all([this.#port.apis(), this.#port.flows()]);
      this.error = undefined;
    } catch (error) {
      this.error = message(error);
    }
  }

  /** Opens the api `id` with an empty row for each var it declares, and reads its request. */
  async open(id: string): Promise<void> {
    const api = this.apis.find((a) => a.id === id);
    if (!api || api.id === this.current?.id) return;
    this.current = api;
    this.vars = api.vars.map((name) => ({ name, value: '' }));
    this.result = undefined;
    this.error = undefined;
    this.request = new ApiRequest(this.#port, api.id);
    await this.request.load();
  }

  addVar(): void {
    this.vars.push({ name: '', value: '' });
  }

  removeVar(index: number): void {
    this.vars.splice(index, 1);
  }

  /** The call the request describes; vars left empty keep the api's defaults. */
  get input(): CallInput | undefined {
    if (!this.current) return undefined;
    const vars = this.vars
      .filter((v) => v.name.trim() && v.value !== '')
      .map((v) => varArg(v.name.trim(), v.value));
    const draft = this.request?.draft;
    const input = { api: this.current.id, vars, keep: this.keep };
    return draft ? { ...input, draft } : input;
  }

  async send(): Promise<void> {
    const input = this.input;
    if (!input || this.sending || this.request?.problem !== undefined) return;
    this.sending = true;
    try {
      const output = await this.#port.call(input);
      const response = parseResponse(output.responseJson);
      const fields = response ? scalarFields(response.body) : [];
      this.result = { input, output, response, fields };
      this.error = undefined;
    } catch (error) {
      if (ConnectError.from(error).code === Code.Aborted) this.request?.stale();
      this.error = message(error);
    } finally {
      this.sending = false;
    }
  }

  /** Saves the last call as the test `testId`; resolves to what was added, or `undefined`. */
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
}
