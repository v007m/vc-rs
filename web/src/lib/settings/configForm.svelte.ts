import { Code, ConnectError, type Client } from '@connectrpc/connect';
import { EditOp, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import {
  CONFIG_PATH,
  draftOf,
  editsBetween,
  profileProblem,
  varProblem,
  type ConfigDraft,
  type PointerEdit,
} from './configEdits.ts';

/** The config as the form opens it, and the digest its edits go over. */
export type ConfigFile = Readonly<{ draft: ConfigDraft; digest: string }>;

/** What `EditFile` gave back: the masked diff and, once written, the new digest and check. */
export type Edited = Readonly<{
  diff: string;
  digest: string;
  written: boolean;
  problems: number;
  checkError?: string;
}>;

/** What the config form asks of the engine: the file as explained, and pointer edits to it. */
export interface ConfigPort {
  open(): Promise<ConfigFile>;
  edit(edits: readonly PointerEdit[], digest: string, write: boolean): Promise<Edited>;
}

const OPS = { set: EditOp.SET, remove: EditOp.REMOVE, insert: EditOp.INSERT } as const;

type ConfigClient = Pick<
  Client<typeof EngineService>,
  'getProject' | 'explainConfig' | 'readFile' | 'editFile'
>;

/** The port over GetProject, ExplainConfig and ReadFile to open, and EditFile to edit. */
export function configPort(client: ConfigClient): ConfigPort {
  return {
    async open() {
      const [project, explained, file] = await Promise.all([
        client.getProject({}),
        client.explainConfig({ profiles: [] }),
        client.readFile({ path: CONFIG_PATH }),
      ]);
      const draft = draftOf(
        project.name,
        explained.values,
        project.profiles,
        project.defaultProfile,
      );
      return { draft, digest: file.digest };
    },
    async edit(edits, digest, write) {
      const wire = edits.map(({ pointer, op, value }) => ({
        pointer,
        op: OPS[op],
        valueJson: value === undefined ? '' : JSON.stringify(value),
      }));
      const response = await client.editFile({ path: CONFIG_PATH, digest, edits: wire, write });
      const { diff, written } = response;
      const edited = {
        diff,
        digest: response.digest,
        written,
        problems: response.diagnostics.length,
      };
      return response.checkError ? { ...edited, checkError: response.checkError } : edited;
    },
  };
}

export type FormState = 'loading' | 'idle' | 'reviewing' | 'saving' | 'conflict';

function empty(): ConfigDraft {
  return {
    project: '',
    fields: { 'defaults.timeout': '', 'defaults.concurrency': '', 'defaults.perHost': '' },
    vars: {},
    profiles: [],
  };
}

/** The config being edited: as read, as edited, the reviewed diff and the save's outcome. */
export class ConfigForm {
  draft: ConfigDraft = $state(empty());
  state: FormState = $state('loading');
  error: string | undefined = $state();
  saved: Edited | undefined = $state();
  #base: ConfigDraft = $state(empty());
  #reviewed: Readonly<{ diff: string; of: string }> | undefined = $state();
  #digest = '';
  readonly edits = $derived(editsBetween(this.#base, this.draft));
  readonly dirty = $derived(this.edits.length > 0);
  /** The diff of the edits as they stand, once reviewed; gone as soon as they change. */
  readonly diff = $derived(
    this.#reviewed?.of === JSON.stringify(this.edits) ? this.#reviewed.diff : undefined,
  );
  readonly #port: ConfigPort;

  constructor(port: ConfigPort) {
    this.#port = port;
  }

  /** Reads the config again, dropping any edits. */
  async load(): Promise<void> {
    this.state = 'loading';
    try {
      const { draft, digest } = await this.#port.open();
      this.#digest = digest;
      this.#base = draft;
      this.draft = structuredClone(draft);
      this.#reviewed = undefined;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.state = 'idle';
    }
  }

  discard(): void {
    this.draft = structuredClone($state.snapshot(this.#base));
  }

  /** Adds a profile, unsaved; gives back why it cannot when it cannot. */
  addProfile(id: string): string | undefined {
    const problem = profileProblem(id, this.draft.profiles);
    if (problem === undefined) this.draft.profiles.push({ id, isDefault: false });
    return problem;
  }

  removeProfile(id: string): void {
    this.draft.profiles = this.draft.profiles.filter((profile) => profile.id !== id);
  }

  /** Makes `id` the profile a run uses when none is named, and no other. */
  makeDefault(id: string): void {
    for (const profile of this.draft.profiles) profile.isDefault = profile.id === id;
  }

  /** Adds a variable, unsaved; gives back why it cannot when it cannot. */
  addVar(name: string, value: string): string | undefined {
    const problem = varProblem(name, this.draft.vars);
    if (problem === undefined) this.draft.vars[name] = value;
    return problem;
  }

  removeVar(name: string): void {
    this.draft.vars = Object.fromEntries(
      Object.entries(this.draft.vars).filter(([key]) => key !== name),
    );
  }

  /** Shows what the edits change, as the engine would write it, without writing it. */
  async review(): Promise<void> {
    const edits = this.edits;
    await this.#send('reviewing', async () => {
      const { diff } = await this.#port.edit(edits, this.#digest, false);
      this.#reviewed = { diff, of: JSON.stringify(edits) };
    });
  }

  /** Writes the edits unless the file changed on disk since it was read, then reads it again. */
  async save(): Promise<void> {
    const edits = this.edits;
    await this.#send('saving', async () => {
      const saved = await this.#port.edit(edits, this.#digest, true);
      await this.load();
      this.saved = saved;
    });
  }

  async #send(state: FormState, send: () => Promise<void>): Promise<void> {
    this.state = state;
    this.saved = undefined;
    this.error = undefined;
    try {
      await send();
      this.state = 'idle';
    } catch (error) {
      const failure = ConnectError.from(error);
      const raced = failure.code === Code.Aborted;
      this.error = raced ? undefined : failure.rawMessage;
      this.state = raced ? 'conflict' : 'idle';
    }
  }
}
