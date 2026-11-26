import { notYet, type Maybe } from '#lib/api/maybe.ts';

/** One completion at the cursor (ui.md §5): an id, var, matcher, pipe or key. */
export type Suggestion = Readonly<{
  label: string;
  kind: 'id' | 'var' | 'matcher' | 'pipe' | 'key';
  /** A short note beside it, such as the type. */
  detail?: string;
  /** The text inserted, when not the label. */
  insert?: string;
}>;

/** The completions for the word that starts at `from`. */
export type Completions = Readonly<{ from: number; items: readonly Suggestion[] }>;

/** What hovering a word shows: its type, its value under the current profile, and the docs. */
export type HoverCard = Readonly<{ title: string; type?: string; value?: string; docs?: string }>;

/**
 * The YAML view's seam over the engine's `Complete` and `Hover` RPCs (ui.md §10, row 4).
 * Offsets are string indexes into `text`, the unsaved buffer of `path`.
 */
export interface EditorAssist {
  complete(path: string, text: string, offset: number): Promise<Maybe<Completions>>;
  hover(path: string, text: string, offset: number): Promise<Maybe<HoverCard | undefined>>;
}

export const assistNotYet = 'Completion and hover need a newer engine.';

/** The engine has no `Complete` or `Hover` yet, so the YAML view offers neither. */
export const noAssist: EditorAssist = {
  complete: () => Promise.resolve(notYet(assistNotYet)),
  hover: () => Promise.resolve(notYet(assistNotYet)),
};
