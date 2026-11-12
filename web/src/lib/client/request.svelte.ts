import {
  bodyProblem,
  checkEdits,
  checkProblem,
  editsBetween,
  formOf,
  maskedNotices,
  type AddedCheck,
  type ApiForm,
} from './apiForm.ts';
import { DefinitionDraft, type DraftPort } from './draft.svelte.ts';
import type { ApiDraft, ClientPort, ExplainedApi, WrittenDefinition } from './port.ts';

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
export class ApiRequest extends DefinitionDraft {
  explained: ExplainedApi | undefined = $state();
  form: ApiForm = $state(empty());
  checks: AddedCheck[] = $state([]);
  #base: ApiForm = $state(empty());
  readonly edits = $derived(
    this.written ? editsBetween(this.written.pointer, this.#base, this.form) : [],
  );
  readonly problem = $derived(bodyProblem(this.form));
  readonly notices = $derived(maskedNotices(this.#base, this.form));
  readonly #port: Pick<ClientPort, 'explainApi'>;

  constructor(port: DraftPort & Pick<ClientPort, 'explainApi'>, api: string) {
    super(port, api);
    this.#port = port;
  }

  /** What a call sends in place of the file: the form's edits, then the added checks. */
  get draft(): ApiDraft | undefined {
    const written = this.written;
    if (!written) return undefined;
    const added = checkEdits(written.pointer, written.node, this.checks);
    const edits = [...this.edits, ...added];
    return edits.length === 0 ? undefined : { path: written.path, digest: written.digest, edits };
  }

  /** The form starts from the api as written; the added checks stay. */
  protected async opened(written: WrittenDefinition): Promise<void> {
    this.explained = await this.#port.explainApi(this.id, []);
    this.#base = formOf(written.node);
    this.form = formOf(written.node);
  }

  /** Explains the request as a call with `vars` sends it now: the draft in place of the file. */
  async explain(vars: readonly string[]): Promise<void> {
    this.explained = await this.#port.explainApi(this.id, vars, this.draft);
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
}
