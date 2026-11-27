/** The top-level key a rule list sits under, with nothing or a comment after it. */
const head = /^alerts:\s*(?<rest>[^#]*?)\s*(#.*)?$/;

/** Whether `line` sits at the top level: a key the next block starts with. */
const topLevel = (line: string): boolean => /^[^\s#]/.test(line);

/** The lines under `alerts:`, up to the next top-level key. */
function block(lines: readonly string[], start: number): string[] {
  const after = lines.slice(start + 1);
  const end = after.findIndex(topLevel);
  return end < 0 ? after : after.slice(0, end);
}

/** Whether `line` starts a list item at `indent`. */
function itemAt(line: string, indent: number): boolean {
  return line.slice(0, indent).trim() === '' && /^-(\s|$)/.test(line.slice(indent));
}

/** Each list item of `lines`, its lines shifted left by the list's indent. */
function items(lines: readonly string[]): string[] {
  const first = lines.find((line) => /^\s*-(\s|$)/.test(line));
  if (first === undefined) return [];
  const indent = first.indexOf('-');
  const rules: string[][] = [];
  for (const line of lines) {
    if (itemAt(line, indent)) rules.push([]);
    rules.at(-1)?.push(line.slice(Math.min(indent, line.length - line.trimStart().length)));
  }
  return rules.map((rule) => rule.join('\n').trimEnd());
}

/**
 * The config's alert rules as it writes them, one text per rule in order, so rule `n` is the
 * `n`th (TestAlert numbers them from 1, D713). No RPC lists the rules, so they are read from
 * the config's text: a block list is split at its items, a flow list is one text.
 */
export function alertRules(text: string): string[] {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => head.test(line));
  if (start < 0) return [];
  const rest = head.exec(lines[start] ?? '')?.groups?.['rest'] ?? '';
  if (rest === '[]') return [];
  if (rest !== '') return [rest];
  return items(block(lines, start));
}
