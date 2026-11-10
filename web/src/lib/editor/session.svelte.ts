import { Code, ConnectError, type Client } from '@connectrpc/connect';
import {
  DefinitionKind,
  Severity,
  type Diagnostic,
  type EngineService,
  type Fix,
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
    return this.fileDiagnostics.filter((d) => d.severity === Severity.ERROR).length;
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
    this.#invalidate();
    this.state = 'loading';
    await this.#attempt(async () => {
      const { text, digest } = await this.#client.readFile({ path });
      this.buffer = { path, original: text, text, digest };
      await this.#check(path);
    });
    this.state = 'idle';
  }

  /** Changes the open file's text and checks it, unsaved, once typing pauses. */
  edit(text: string): void {
    if (!this.buffer) return;
    this.buffer = { ...this.buffer, text };
    const { path } = this.buffer;
    const version = this.#invalidate();
    this.#timer = setTimeout(
      () => void this.#attempt(() => this.#checkUnsaved(version, path, text)),
      this.#checkDelayMs,
    );
  }

  /** Drops the edits since the last read or save. */
  revert(): void {
    if (this.buffer) this.edit(this.buffer.original);
  }

  /** Saves the open file unless it changed on disk since it was read. */
  async save(): Promise<void> {
    const buffer = this.buffer;
    if (!buffer || !this.canSave) return;
    this.#invalidate();
    this.state = 'saving';
    try {
      const { path, text, digest } = buffer;
      const response = await this.#client.writeFile({ path, text, expectedDigest: digest });
      this.buffer = { ...buffer, original: text, digest: response.digest };
      this.diagnostics = response.diagnostics;
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
      this.buffer = { ...buffer, digest };
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

  async #check(path: string): Promise<void> {
    this.diagnostics = (await this.#client.check({ paths: [path] })).diagnostics;
  }

  /** Checks `text` in place of the file on disk; the result is dropped if the text moved on. */
  async #checkUnsaved(version: number, path: string, text: string): Promise<void> {
    const { diagnostics } = await this.#client.check({ paths: [path], overlay: { [path]: text } });
    if (version === this.#version) this.diagnostics = diagnostics;
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
