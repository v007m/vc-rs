import { Severity, type Diagnostic, type Fix } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { applyEdits, indexAt, type ByteEdit } from './text.ts';

/** A diagnostic placed in the text, as the YAML view underlines it. */
export type Mark = Readonly<{
  from: number;
  to: number;
  severity: 'error' | 'warning' | 'info';
  code: string;
  message: string;
  /** The certain fixes, each a quick fix in the squiggle's tooltip. */
  fixes: readonly Fix[];
}>;

const severities: Partial<Record<Severity, Mark['severity']>> = {
  [Severity.ERROR]: 'error',
  [Severity.WARNING]: 'warning',
};

/**
 * The diagnostics of `path` placed in `text`. A span with no end marks its start; one with no line
 * marks the first line, so every diagnostic of the file shows somewhere.
 */
export function marks(text: string, path: string, diagnostics: readonly Diagnostic[]): Mark[] {
  return diagnostics.flatMap((d) => {
    const span = d.primary?.span;
    if (span?.file !== path) return [];
    const from = indexAt(text, span.startLine || 1, span.startColumn || 1);
    const end = span.endLine ? indexAt(text, span.endLine, span.endColumn || 1) : from;
    return [
      {
        from,
        to: Math.max(from, end),
        severity: severities[d.severity] ?? 'info',
        code: d.code,
        message: d.message,
        fixes: d.fixes.filter((fix) => fix.certain),
      },
    ];
  });
}

/** What Fix all did: the new text and how many fixes it applied. */
export type FixedText = Readonly<{ text: string; applied: number }>;

/**
 * `text` with each diagnostic's first certain fix applied (Fix all). A fix that would touch
 * another file, or overlap a fix taken before it, is left for the next check to offer again.
 */
export function fixAll(text: string, path: string, diagnostics: readonly Diagnostic[]): FixedText {
  const taken: ByteEdit[] = [];
  let applied = 0;
  for (const fix of diagnostics.flatMap((d) => d.fixes.find((f) => f.certain) ?? [])) {
    if (fix.edits.some((edit) => edit.file !== path)) continue;
    if (fix.edits.some((edit) => taken.some((t) => overlaps(t, edit)))) continue;
    taken.push(...fix.edits);
    applied += 1;
  }
  return { text: applyEdits(text, taken), applied };
}

function overlaps(a: ByteEdit, b: ByteEdit): boolean {
  return (a.start < b.end && b.start < a.end) || a.start === b.start;
}
