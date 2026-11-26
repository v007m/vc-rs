import { notYet, type Maybe } from '#lib/api/maybe.ts';
import { noAssist, type EditorAssist } from './assist.ts';

/** What `vc new` can make (ui.md §5): a service folder, an api file or a test file. */
export type Template = 'service' | 'api' | 'test';

/** One file a rename would change, before and after. */
export type FileChange = Readonly<{ path: string; before: string; after: string }>;

/**
 * The editor's file operations (ui.md §5), over RPCs the engine has not got yet: `vc new`,
 * `vc rename` with a preview of every file it changes, `vc fmt` of the unsaved text, and delete to
 * the OS trash after listing the tests that use the file.
 */
export interface FileOps {
  /** Makes `name` from `template`, in `service` unless it is a service; gives the new file's path. */
  create(template: Template, name: string, service: string): Promise<Maybe<string>>;
  /** The files that renaming or moving `from` to `to` would change, references included. */
  previewRename(from: string, to: string): Promise<Maybe<readonly FileChange[]>>;
  rename(from: string, to: string): Promise<Maybe<void>>;
  /** `text`, the unsaved buffer of `path`, as `vc fmt` would write it. */
  format(path: string, text: string): Promise<Maybe<string>>;
  /** The full IDs of the tests outside `path` that refer to something in it. */
  references(path: string): Promise<Maybe<readonly string[]>>;
  trash(path: string): Promise<Maybe<void>>;
}

/** One planned instance: a test, or one example row of it, with its merged expects. */
export type PlannedInstance = Readonly<{ id: string; name: string; expects: readonly string[] }>;

/** One lifecycle step in run order, such as `beforeAll` of a file or a test's cleanup. */
export type PlannedStep = Readonly<{ phase: string; target: string }>;

/** What `vc plan --explain` shows for a selection (ui.md §5). */
export type PlanPreview = Readonly<{
  instances: readonly PlannedInstance[];
  order: readonly PlannedStep[];
}>;

export interface PlanPort {
  plan(paths: readonly string[]): Promise<Maybe<PlanPreview>>;
}

/** The editor's seams over RPCs the engine has not got yet (D538). */
export type EditorSeams = Readonly<{ assist: EditorAssist; files: FileOps; plan: PlanPort }>;

export const filesNotYet = 'File operations need a newer engine.';
export const planNotYet = 'The plan preview needs a newer engine.';

const missing = () => Promise.resolve(notYet(filesNotYet));

/** Today's engine: no completion, hover, file operations or plan; each shows why. */
export const noEditorSeams: EditorSeams = {
  assist: noAssist,
  files: {
    create: missing,
    previewRename: missing,
    rename: missing,
    format: missing,
    references: missing,
    trash: missing,
  },
  plan: { plan: () => Promise.resolve(notYet(planNotYet)) },
};
