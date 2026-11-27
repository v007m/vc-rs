import { notYet, type Maybe } from '#lib/api/maybe.ts';
import { noAssist, type EditorAssist } from './assist.ts';

/** What `vc new file` can make (ui.md §5): a test file, a load file, or a service's api file. */
export type Template = 'test' | 'load' | 'service';

/** One file an operation would change, before and after; a new file has no `before`. */
export type FileChange = Readonly<{
  path: string;
  before: string;
  after: string;
  movedFrom?: string;
}>;

/** What an operation would write, and the digest its write must send back (D683). */
export type FilePreview = Readonly<{ files: readonly FileChange[]; digest: string }>;

/** What refers to a file from elsewhere, before it goes to the Trash. */
export type FileUses = Readonly<{
  /** `users.flow#placesOrder · services/users/flow.test.yaml:12`, one per reference. */
  uses: readonly string[];
  /** Files that do not parse, so were not searched. */
  unread: readonly string[];
}>;

/**
 * The editor's file operations (ui.md §5, D683): `vc new file`, `vc rename` of a file ID with a
 * preview of every file it changes, `vc fmt` of the unsaved text, and Move to Trash after
 * listing what refers to the file.
 */
export interface FileOps {
  /** Makes `name` from `template`, in `service` unless it is a service; gives the new file's path. */
  create(template: Template, name: string, service: string): Promise<Maybe<string>>;
  /** The files that renaming the file ID `from` to `to` would change, references included. */
  previewRename(from: string, to: string): Promise<Maybe<FilePreview>>;
  /** Renames `from` to `to` unless a file changed since the preview gave `digest`. */
  rename(from: string, to: string, digest: string): Promise<Maybe<readonly FileChange[]>>;
  /** `text`, the unsaved buffer of `path`, as `vc fmt` would write it. */
  format(path: string, text: string): Promise<Maybe<string>>;
  references(path: string): Promise<Maybe<FileUses>>;
  /** Moves `path` to the Trash unless it changed since it was read as `digest`; gives where. */
  trash(path: string, digest: string): Promise<Maybe<string>>;
}

/** What `vc plan` prints for a selection (D682, D655): its line, its tree and its notices. */
export type PlanPreview = Readonly<{
  line: string;
  tree: string;
  instances: number;
  notices: readonly string[];
}>;

export interface PlanPort {
  plan(paths: readonly string[]): Promise<Maybe<PlanPreview>>;
}

/** `vc explain` of a code, test or api (D679): Markdown for a code's page, else plain text. */
export type Explained = Readonly<{ text: string; markdown: boolean; notices: readonly string[] }>;

export interface ExplainPort {
  explain(target: string): Promise<Maybe<Explained>>;
}

/**
 * The engine's fixes of a saved file (D680, D652): the file's text after every certain fix of
 * `codes` (all codes when empty), or `undefined` when there is nothing to fix.
 */
export interface FixPort {
  fix(path: string, codes: readonly string[]): Promise<Maybe<string | undefined>>;
}

/** The editor's seams over the engine's newer RPCs (D538). */
export type EditorSeams = Readonly<{
  assist: EditorAssist;
  files: FileOps;
  plan: PlanPort;
  explain: ExplainPort;
  fixes: FixPort;
}>;

export const filesNotYet = 'File operations need a newer engine.';
export const planNotYet = 'The plan preview needs a newer engine.';
export const explainNotYet = 'Explain needs a newer engine.';
export const fixesNotYet = 'Fixing through the engine needs a newer engine.';

const missing = () => Promise.resolve(notYet(filesNotYet));

/** The new path of the file a rename moved from `path`, else the first file it changed. */
export function movedTo(files: readonly FileChange[], path: string): string {
  return files.find((f) => f.movedFrom === path)?.path ?? files[0]?.path ?? path;
}

/** An engine without them: no completion, hover, file operations, plan, explain or fixes. */
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
  explain: { explain: () => Promise.resolve(notYet(explainNotYet)) },
  fixes: { fix: () => Promise.resolve(notYet(fixesNotYet)) },
};
