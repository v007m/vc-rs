import type { Client } from '@connectrpc/connect';
import { notYet, unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import { CompletionKind, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** One completion at the cursor (ui.md §5): an id, var, matcher, value or key (D650). */
export type Suggestion = Readonly<{
  label: string;
  kind: 'id' | 'var' | 'matcher' | 'pipe' | 'key' | 'value';
  /** A short note beside it, such as the type, or where a var is set (never its value). */
  detail?: string;
  /** A key's docs. */
  doc?: string;
  /** The text inserted, when not the label. */
  insert?: string;
}>;

/** The completions for the word that starts at `from`. */
export type Completions = Readonly<{ from: number; items: readonly Suggestion[] }>;

/** What hovering a key shows: its docs, then the reference it names explained, as Markdown. */
export type HoverCard = Readonly<{ markdown: string }>;

/**
 * The YAML view's seam over the engine's `Complete` and `Hover` RPCs (ui.md §10, row 4).
 * Offsets are string indexes into `text`, the unsaved buffer of `path`: UTF-16 code units, as the
 * engine takes them (D681).
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

const kinds: Readonly<Record<CompletionKind, Suggestion['kind']>> = {
  [CompletionKind.UNSPECIFIED]: 'key',
  [CompletionKind.KEY]: 'key',
  [CompletionKind.VALUE]: 'value',
  [CompletionKind.REFERENCE]: 'id',
  [CompletionKind.VAR]: 'var',
  [CompletionKind.MATCHER]: 'matcher',
};

/** Where the word before `offset` starts: the text the engine's completions replace. */
export function wordStart(text: string, offset: number): number {
  const word = /[\w.#${-]*$/.exec(text.slice(0, offset));
  return offset - (word?.[0].length ?? 0);
}

/** Completion and hover over the engine client's `Complete` and `Hover`. */
export function engineAssist(
  client: Pick<Client<typeof EngineService>, 'complete' | 'hover'>,
): EditorAssist {
  return {
    complete: (path, text, offset) =>
      unlessMissing(assistNotYet, async () => {
        const { items } = await client.complete({ path, text, offset });
        return {
          from: wordStart(text, offset),
          items: items.map((item) => ({
            label: item.label,
            kind: kinds[item.kind] ?? 'key',
            ...(item.detail ? { detail: item.detail } : {}),
            ...(item.doc ? { doc: item.doc } : {}),
          })),
        };
      }),
    hover: (path, text, offset) =>
      unlessMissing(assistNotYet, async () => {
        const { markdown } = await client.hover({ path, text, offset });
        return markdown ? { markdown } : undefined;
      }),
  };
}
