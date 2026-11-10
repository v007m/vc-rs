import { Code, ConnectError, type Client } from '@connectrpc/connect';
import {
  DefinitionKind,
  Severity,
  type Diagnostic,
  type EngineService,
  type Fix,
  type WriteFileResponse,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { applyEdits } from './text.ts';

/** The part of the engine client the editor needs, so tests can pass a fake. */
export type EditorClient = Pick<
  Client<typeof EngineService>,
  'listDefinitions' | 'readFile' | 'writeFile' | 'check'
>;

/** An open file: the text on disk when it was read or saved, and the text being edited. */
export type Buffer = Readonly<{ path: string; original: string; text: string; digest: string }>;

/** What the editor is doing; `conflict` means the file changed on disk since it was read. */
export type EditorState = 'idle' | 'loading' | 'saving' | 'conflict';

/** The Editor screen's one open file, its diagnostics, and the project's file list. */
export class EditorSession {
  files: readonly string[] = $state([]);
  buffer: Buffer | undefined = $state();
  diagnostics: readonly Diagnostic[] = $state([]);
  state: EditorState = $state('idle');
  error: string | undefined = $state();
  readonly #client: EditorClient;
  readonly #checkDelayMs: number;
  /** Bumped by every edit, open and save, so a check of older text cannot land. */
  #version = 0;
  /** Bumped by every open, so the read of a file no longer chosen cannot land. */
  #opening = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;

  /** `checkDelayMs` is how long typing must pause before the unsaved text is checked. */
  constructor(client: EditorClient, checkDelayMs = 300) {
    this.#client = client;
    this.#checkDelayMs = checkDelayMs;
  }

  get dirty(): boolean {
    return !!this.buffer && this.buffer.text !== this.buffer.original;
  }

  /** The diagnostics whose main span is in the open file. */
  get fileDiagnostics(): Diagnostic[] {
    return this.diagnostics.filter((d) => d.primary?.span?.file === this.buffer?.path);
  }

  /** How many errors the open file has; Save is blocked while there are any (R44). */
  get errorCount(): number {
    return this.diagnostics.filter((d) => isErrorIn(d, this.buffer?.path)).length;
  }

  get canSave(): boolean {
    return this.dirty && this.errorCount === 0 && this.state !== 'saving';
  }

  async loadFiles(): Promise<void> {
    await this.#attempt(async () => {
      const response = await this.#client.listDefinitions({ kind: DefinitionKind.FILES });
      this.files = response.definitions.map((d) => d.path).sort();
    });
  }

  /** Opens `path`, dropping unsaved changes to the open file; the screen asks first. */
  async open(path: string): Promise<void> {
    const ticket = ++this.#opening;
    this.#invalidate();
    this.state = 'loading';
    try {
      const { text, digest } = await this.#client.readFile({ path });
      if (ticket !== this.#opening) return;
      this.buffer = { path, original: text, text, digest };
      await this.#checkAs(this.#invalidate(), path);
      this.error = undefined;
    } catch (error) {
      if (ticket === this.#opening) this.error = ConnectError.from(error).rawMessage;
    } finally {
      if (ticket === this.#opening) this.state = 'idle';
    }
  }

  /** Changes the open file's text and checks it, unsaved, once typing pauses. */
  edit(text: string): void {
    if (!this.buffer) return;
    this.buffer = { ...this.buffer, text };
    const { path } = this.buffer;
    const version = this.#invalidate();
    const check = async () => void (await this.#checkAs(version, path, text));
    this.#timer = setTimeout(() => void this.#attempt(check), this.#checkDelayMs);
  }

  /** Drops the edits since the last read or save. */
  revert(): void {
    if (this.buffer) this.edit(this.buffer.original);
  }

  /**
   * Saves the open file unless it changed on disk since it was read, or its text has errors: the
   * text is checked first, since the last check may predate the last keystroke (R44).
   */
  async save(): Promise<void> {
    const buffer = this.buffer;
    if (!buffer || !this.canSave) return;
    const { path, text, digest } = buffer;
    const version = this.#invalidate();
    this.state = 'saving';
    try {
      const checked = await this.#checkAs(version, path, text);
      if (!checked.some((d) => isErrorIn(d, path))) {
        const response = await this.#client.writeFile({ path, text, expectedDigest: digest });
        this.#saved(version, buffer, response);
      }
      this.error = undefined;
      this.state = 'idle';
    } catch (error) {
      const failure = ConnectError.from(error);
      this.state = failure.code === Code.Aborted ? 'conflict' : 'idle';
      this.error = failure.code === Code.Aborted ? undefined : failure.rawMessage;
    }
  }

  /** After a conflict: drops the edits and reads the file as it is on disk now. */
  async reload(): Promise<void> {
    if (this.buffer) await this.open(this.buffer.path);
  }

  /** After a conflict: saves the edits over the file as it is on disk now. */
  async keepMine(): Promise<void> {
    const buffer = this.buffer;
    if (!buffer) return;
    await this.#attempt(async () => {
      const { digest } = await this.#client.readFile({ path: buffer.path });
      if (this.buffer?.path === buffer.path) this.buffer = { ...this.buffer, digest };
      this.state = 'idle';
      await this.save();
    });
  }

  /** Applies the fix's edits to the open file, unsaved. */
  applyFix(fix: Fix): void {
    const buffer = this.buffer;
    if (!buffer) return;
    const edits = fix.edits.filter((e) => e.file === buffer.path);
    this.edit(applyEdits(buffer.text, edits));
  }

  /**
   * Checks `path`, with `text` in place of the file on disk when given. The diagnostics are
   * returned, and shown unless the text moved on since `version`.
   */
  async #checkAs(version: number, path: string, text?: string): Promise<Diagnostic[]> {
    const overlay = text === undefined ? {} : { [path]: text };
    const { diagnostics } = await this.#client.check({ paths: [path], overlay });
    if (version === this.#version) this.diagnostics = diagnostics;
    return diagnostics;
  }

  /**
   * Records that `saved` is on disk. Only the live buffer's disk side changes, so keys typed
   * while the save was in flight stay, unsaved; the write's diagnostics land unless they did.
   */
  #saved(version: number, saved: Buffer, response: WriteFileResponse): void {
    const live = this.buffer;
    if (live?.path === saved.path)
      this.buffer = { ...live, original: saved.text, digest: response.digest };
    if (version === this.#version) this.diagnostics = response.diagnostics;
  }

  /** Cancels the pending check and makes any check in flight stale. */
  #invalidate(): number {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    this.#version += 1;
    return this.#version;
  }

  async #attempt(step: () => Promise<void>): Promise<void> {
    try {
      await step();
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }
}

/** Whether `d` is an error whose main span is in `path`. */
function isErrorIn(d: Diagnostic, path: string | undefined): boolean {
  return d.severity === Severity.ERROR && d.primary?.span?.file === path;
}
