/** A replacement of the bytes `start..end` of a file, as engine fixes give it. */
export type ByteEdit = Readonly<{ start: number; end: number; newText: string }>;

const encoder = new TextEncoder();

/** The string index of 1-based `line` and character `column`, as diagnostic spans give them. */
export function indexAt(text: string, line: number, column: number): number {
  let index = 0;
  for (let n = 1; n < line; n += 1) {
    const next = text.indexOf('\n', index);
    if (next < 0) return text.length;
    index = next + 1;
  }
  const end = text.indexOf('\n', index);
  const rest = text.slice(index, end < 0 ? undefined : end);
  const chars = Array.from(rest).slice(0, Math.max(0, column - 1));
  return index + chars.join('').length;
}

/** The string index of UTF-8 byte offset `byte` in `text`. */
export function indexOfByte(text: string, byte: number): number {
  let bytes = 0;
  let index = 0;
  for (const char of text) {
    if (bytes >= byte) break;
    bytes += encoder.encode(char).length;
    index += char.length;
  }
  return index;
}

/** `text` with `edits` applied; they must not overlap. */
export function applyEdits(text: string, edits: readonly ByteEdit[]): string {
  return [...edits]
    .sort((a, b) => b.start - a.start)
    .reduce(
      (out, edit) =>
        out.slice(0, indexOfByte(out, edit.start)) +
        edit.newText +
        out.slice(indexOfByte(out, edit.end)),
      text,
    );
}

/** One line of a diff: kept, added or removed, with its line numbers on each side. */
export type DiffLine = Readonly<{
  kind: 'same' | 'add' | 'del';
  text: string;
  oldNo?: number;
  newNo?: number;
}>;

/** The longest-common-subsequence table of `a` and `b`, read from the end. */
function lcsTable(a: readonly string[], b: readonly string[]): Uint32Array[] {
  const table = Array.from({ length: a.length + 1 }, () => new Uint32Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i -= 1) {
    const row = table[i] as Uint32Array;
    const below = table[i + 1] as Uint32Array;
    for (let j = b.length - 1; j >= 0; j -= 1) {
      row[j] = a[i] === b[j] ? (below[j + 1] ?? 0) + 1 : Math.max(below[j] ?? 0, row[j + 1] ?? 0);
    }
  }
  return table;
}

/** The line diff of `before` and `after`: a real LCS diff, so moved blocks read naturally. */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.split('\n');
  const b = after.split('\n');
  const table = lcsTable(a, b);
  const out: DiffLine[] = [];
  let [i, j] = [0, 0];
  while (i < a.length || j < b.length) {
    const take = (table[i + 1]?.[j] ?? 0) >= (table[i]?.[j + 1] ?? 0);
    if (i < a.length && j < b.length && a[i] === b[j]) {
      out.push({ kind: 'same', text: a[i] ?? '', oldNo: ++i, newNo: ++j });
    } else if (i < a.length && (j >= b.length || take)) {
      out.push({ kind: 'del', text: a[i] ?? '', oldNo: ++i });
    } else {
      out.push({ kind: 'add', text: b[j] ?? '', newNo: ++j });
    }
  }
  return out;
}

/** Unchanged lines a diff view folds away, between the kept context lines. */
export type DiffGap = Readonly<{ kind: 'gap'; count: number }>;

/** The diff with runs of unchanged lines folded to `context` lines around each change. */
export function foldDiff(lines: readonly DiffLine[], context = 3): (DiffLine | DiffGap)[] {
  const near = lines.map((_, i) =>
    lines.slice(Math.max(0, i - context), i + context + 1).some((l) => l.kind !== 'same'),
  );
  const out: (DiffLine | DiffGap)[] = [];
  lines.forEach((line, i) => {
    if (near[i]) return void out.push(line);
    const last = out.at(-1);
    if (last?.kind === 'gap') out[out.length - 1] = { kind: 'gap', count: last.count + 1 };
    else out.push({ kind: 'gap', count: 1 });
  });
  return out;
}
