import { Code, ConnectError } from '@connectrpc/connect';
import type { Edited } from '#lib/settings/configForm.svelte.ts';
import {
  bodyProblem,
  checkEdits,
  checkProblem,
  editsBetween,
  formOf,
  type AddedCheck,
  type ApiForm,
} from './apiForm.ts';
import type { ApiDraft, ClientPort, ExplainedApi } from './port.ts';

export type RequestState = 'loading' | 'idle' | 'reviewing' | 'saving' | 'conflict';

/** The lists of rows the form has. */
export type RowList = 'query' | 'headers';

function empty(): ApiForm {
  return { method: 'GET', path: '', query: [], headers: [], bodyMode: 'none', body: '' };
}

/**
 * The open api as the Client edits it (D245): the form over the api as its file writes it, the
 * checks added for this request alone, and saving the form's edits back to the file. A call
 * sends both as a draft; nothing is written until a save, and a save never writes the checks.
 */
export class ApiRequest {
  explained: ExplainedApi | undefined = $state();
  form: ApiForm = $state(empty());
  checks: AddedCheck[] = $state([]);
  state: RequestState = $state('loading');
  error: string | undefined = $state();
  saved: Edited | undefined = $state();
  #base: ApiForm = $state(empty());
  #reviewed: Readonly<{ diff: string; of: string }> | undefined = $state();
  readonly edits = $derived(
    this.explained ? editsBetween(this.explained.pointer, this.#base, this.form) : [],
  );
  readonly dirty = $derived(this.edits.length > 0);
  /** Why the request cannot be sent or saved as it stands, if it cannot. */
  readonly problem = $derived(bodyProblem(this.form));
  /** The masked diff of the edits as they stand, once reviewed; gone when they change. */
  readonly diff = $derived(
    this.#reviewed?.of === JSON.stringify(this.edits) ? this.#reviewed.diff : undefined,
  );
  readonly #port: Pick<ClientPort, 'explainApi' | 'editApi'>;
  readonly #api: string;

  constructor(port: Pick<ClientPort, 'explainApi' | 'editApi'>, api: string) {
    this.#port = port;
    this.#api = api;
  }

  /** What a call sends in place of the file: the form's edits, then the added checks. */
  get draft(): ApiDraft | undefined {
    const explained = this.explained;
    if (!explained) return undefined;
    const added = checkEdits(explained.pointer, explained.api, this.checks);
    const edits = [...this.edits, ...added];
    return edits.length === 0
      ? undefined
      : { path: explained.path, digest: explained.digest, edits };
  }

  /** Reads the api again, dropping the form's edits; the added checks stay. */
  async load(): Promise<void> {
    this.state = 'loading';
    try {
      const explained = await this.#port.explainApi(this.#api, []);
      this.explained = explained;
      this.#base = formOf(explained.api);
      this.form = formOf(explained.api);
      this.#reviewed = undefined;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.state = 'idle';
    }
  }

  discard(): void {
    this.form = structuredClone($state.snapshot(this.#base));
  }

  addRow(list: RowList): void {
    this.form[list].push({ name: '', value: '', on: true });
  }

  removeRow(list: RowList, at: number): void {
    this.form[list].splice(at, 1);
  }

  /** Adds a check for this request; gives back why it cannot when it cannot. */
  addCheck(check: AddedCheck): string | undefined {
    const problem = checkProblem(check);
    if (problem === undefined) this.checks.push(check);
    return problem;
  }

  removeCheck(at: number): void {
    this.checks.splice(at, 1);
  }

  /** The file changed on disk since it was read: a call or save over it was refused. */
  stale(): void {
    this.state = 'conflict';
  }

  /** Shows what a save would write, masked, without writing it. */
  async review(): Promise<void> {
    const edits = this.edits;
    await this.#send('reviewing', async (draft) => {
      const { diff } = await this.#port.editApi(draft, false);
      this.#reviewed = { diff, of: JSON.stringify(edits) };
    });
  }

  /** Writes the form's edits unless the file changed since it was read, then reads it again. */
  async save(): Promise<void> {
    await this.#send('saving', async (draft) => {
      const saved = await this.#port.editApi(draft, true);
      await this.load();
      this.saved = saved;
    });
  }

  async #send(state: RequestState, send: (draft: ApiDraft) => Promise<void>): Promise<void> {
    const explained = this.explained;
    if (!explained || this.problem !== undefined) return;
    this.state = state;
    this.saved = undefined;
    this.error = undefined;
    try {
      await send({ path: explained.path, digest: explained.digest, edits: this.edits });
      this.state = 'idle';
    } catch (error) {
      const failure = ConnectError.from(error);
      const raced = failure.code === Code.Aborted;
      this.error = raced ? undefined : failure.rawMessage;
      this.state = raced ? 'conflict' : 'idle';
    }
  }
}
