import { Code, ConnectError } from '@connectrpc/connect';
import type { PointerEdit } from '#lib/settings/configEdits.ts';
import type { Edited } from '#lib/settings/configForm.svelte.ts';
import type { ApiDraft, ClientPort, WrittenDefinition } from './port.ts';

export type DraftState = 'loading' | 'idle' | 'reviewing' | 'saving' | 'conflict';

/** What a draft asks of the engine: the definition as written, and pointer edits to its file. */
export type DraftPort = Pick<ClientPort, 'readDefinition' | 'editFile'>;

/**
 * An api or test the Client edits as its file writes it (D245, D251). A subclass turns the
 * written node into its form and the form back into pointer edits; this reads it, shows the
 * edits as a masked diff, and writes them unless the file changed since it was read.
 */
export abstract class DefinitionDraft {
  written: WrittenDefinition | undefined = $state();
  state: DraftState = $state('loading');
  error: string | undefined = $state();
  saved: Edited | undefined = $state();
  #reviewed: Readonly<{ diff: string; of: string }> | undefined = $state();
  /** The form's edits to the file as they stand. */
  abstract readonly edits: readonly PointerEdit[];
  /** Why the form cannot be sent or saved as it stands, if it cannot. */
  abstract readonly problem: string | undefined;
  readonly id: string;
  readonly #port: DraftPort;

  constructor(port: DraftPort, id: string) {
    this.#port = port;
    this.id = id;
  }

  get dirty(): boolean {
    return this.edits.length > 0;
  }

  /** The masked diff of the edits as they stand, once reviewed; gone when they change. */
  get diff(): string | undefined {
    const reviewed = this.#reviewed;
    return reviewed?.of === JSON.stringify(this.edits) ? reviewed.diff : undefined;
  }

  /** Starts the form from the definition as `written`. */
  protected abstract opened(written: WrittenDefinition): Promise<void> | void;

  /** Reads the definition again, dropping the form's edits. */
  async load(): Promise<void> {
    this.state = 'loading';
    try {
      const written = await this.#port.readDefinition(this.id);
      await this.opened(written);
      this.written = written;
      this.#reviewed = undefined;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.state = 'idle';
    }
  }

  /** The file changed on disk since it was read: a call or save over it was refused. */
  stale(): void {
    this.state = 'conflict';
  }

  /** Shows what a save would write, masked, without writing it. */
  async review(): Promise<void> {
    const edits = this.edits;
    await this.#send('reviewing', async (draft) => {
      const { diff } = await this.#port.editFile(draft, false);
      this.#reviewed = { diff, of: JSON.stringify(edits) };
    });
  }

  /** Writes the form's edits unless the file changed since it was read, then reads it again. */
  async save(): Promise<void> {
    await this.#send('saving', async (draft) => {
      const saved = await this.#port.editFile(draft, true);
      await this.load();
      this.saved = saved;
    });
  }

  async #send(state: DraftState, send: (draft: ApiDraft) => Promise<void>): Promise<void> {
    const written = this.written;
    if (!written || this.problem !== undefined) return;
    this.state = state;
    this.saved = undefined;
    this.error = undefined;
    try {
      await send({ path: written.path, digest: written.digest, edits: this.edits });
      this.state = 'idle';
    } catch (error) {
      const failure = ConnectError.from(error);
      const raced = failure.code === Code.Aborted;
      this.error = raced ? undefined : failure.rawMessage;
      this.state = raced ? 'conflict' : 'idle';
    }
  }
}
