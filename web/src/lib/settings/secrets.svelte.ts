import { Code, ConnectError, type Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { declare, nameProblem, parseSchema, SCHEMA_PATH, undeclare } from './envSchema.ts';

/** `.env.schema` as read; no digest when there is no file yet. */
export type SchemaFile = Readonly<{ text: string; digest?: string }>;

/** What a save left: the digest to save over next and how many problems the project has. */
export type SchemaSaved = Readonly<{ digest: string; problems: number; checkError?: string }>;

/** What the secrets section asks of the engine: `.env.schema` and nothing else (D247). */
export interface SchemaPort {
  read(): Promise<SchemaFile>;
  write(text: string, digest?: string): Promise<SchemaSaved>;
}

/** The port over the engine client's guarded ReadFile and WriteFile. */
export function schemaPort(
  client: Pick<Client<typeof EngineService>, 'readFile' | 'writeFile'>,
): SchemaPort {
  return {
    async read() {
      try {
        const { text, digest } = await client.readFile({ path: SCHEMA_PATH });
        return { text, digest };
      } catch (error) {
        if (ConnectError.from(error).code === Code.NotFound) return { text: '' };
        throw error;
      }
    },
    async write(text, digest) {
      const request = { path: SCHEMA_PATH, text, ...(digest ? { expectedDigest: digest } : {}) };
      const response = await client.writeFile(request);
      const saved = { digest: response.digest, problems: response.diagnostics.length };
      return response.checkError ? { ...saved, checkError: response.checkError } : saved;
    },
  };
}

export type SchemaState = 'loading' | 'idle' | 'saving' | 'conflict';

/** The secret names being edited: the text as read, as edited, and the save's outcome. */
export class SecretSchema {
  original = $state('');
  text = $state('');
  state: SchemaState = $state('loading');
  error: string | undefined = $state();
  saved: SchemaSaved | undefined = $state();
  readonly entries = $derived(parseSchema(this.text));
  readonly dirty = $derived(this.text !== this.original);
  #digest: string | undefined;
  readonly #port: SchemaPort;

  constructor(port: SchemaPort) {
    this.#port = port;
  }

  /** Reads the file again, dropping any edits. */
  async load(): Promise<void> {
    this.state = 'loading';
    try {
      const { text, digest } = await this.#port.read();
      this.#digest = digest;
      this.original = text;
      this.text = text;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.state = 'idle';
    }
  }

  /** Declares `name`, unsaved; gives back why it cannot when it cannot. */
  add(name: string): string | undefined {
    const problem = nameProblem(name, this.entries);
    if (problem === undefined) this.text = declare(this.text, name);
    return problem;
  }

  remove(name: string): void {
    this.text = undeclare(this.text, name);
  }

  discard(): void {
    this.text = this.original;
  }

  /** Writes the edits unless the file changed on disk since it was read. */
  async save(): Promise<void> {
    const text = this.text;
    this.state = 'saving';
    this.saved = undefined;
    try {
      this.saved = await this.#port.write(text, this.#digest);
      this.#digest = this.saved.digest;
      this.original = text;
      this.error = undefined;
      this.state = 'idle';
    } catch (error) {
      const failure = ConnectError.from(error);
      const raced = failure.code === Code.Aborted || failure.code === Code.AlreadyExists;
      this.error = raced ? undefined : failure.rawMessage;
      this.state = raced ? 'conflict' : 'idle';
    }
  }
}
