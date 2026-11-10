/** Where the project declares its secret names (D163, D247). */
export const SCHEMA_PATH = '.env.schema';

/**
 * One name `.env.schema` declares, as Settings shows it: its `# @…` annotations and the other
 * comment lines just above it. A value after `=` is never part of it (D247).
 */
export type SchemaEntry = Readonly<{
  name: string;
  annotations: readonly string[];
  notes: readonly string[];
}>;

const declaration = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=/;
const comment = /^\s*#/;
/** A divider such as varlock's `# ---` ends the comment block that belongs to a name. */
const divider = /^\s*#\s*-{3,}/;

/** The lines of the comment block attached to line `at`: no blank line between, no divider. */
function attached(lines: readonly string[], at: number): number {
  let start = at;
  while (start > 0) {
    const line = lines[start - 1] ?? '';
    if (!comment.test(line) || divider.test(line)) break;
    start -= 1;
  }
  return start;
}

function entry(name: string, block: readonly string[]): SchemaEntry {
  const texts = block.map((line) => line.replace(comment, '').trim());
  return {
    name,
    annotations: texts.flatMap((t) => (t.startsWith('@') ? t.split(/\s+/) : [])),
    notes: texts.filter((t) => t !== '' && !t.startsWith('@')),
  };
}

/** The names `text` declares, in file order, each once. */
export function parseSchema(text: string): SchemaEntry[] {
  const lines = text.split('\n');
  const seen = new Set<string>();
  const entries: SchemaEntry[] = [];
  lines.forEach((line, at) => {
    const name = declaration.exec(line)?.[1];
    if (name === undefined || seen.has(name)) return;
    seen.add(name);
    entries.push(entry(name, lines.slice(attached(lines, at), at)));
  });
  return entries;
}

/** Why `name` cannot be declared next to `entries`, or undefined when it can. */
export function nameProblem(name: string, entries: readonly SchemaEntry[]): string | undefined {
  if (!/^[A-Z][A-Z0-9_]*$/.test(name))
    return 'Use capital letters, digits and underscores, starting with a letter: SHOP_TOKEN.';
  if (entries.some((e) => e.name === name)) return `${name} is declared already.`;
  return undefined;
}

/** `text` with `name` declared on a line of its own at the end, with no value. */
export function declare(text: string, name: string): string {
  const base = text === '' || text.endsWith('\n') ? text : `${text}\n`;
  return `${base}${name}=\n`;
}

/** `text` without the lines declaring `name`, nor the comment block attached to each. */
export function undeclare(text: string, name: string): string {
  const lines = text.split('\n');
  const dropped = new Set<number>();
  lines.forEach((line, at) => {
    if (declaration.exec(line)?.[1] !== name) return;
    for (let i = attached(lines, at); i <= at; i += 1) dropped.add(i);
  });
  return lines.filter((_, at) => !dropped.has(at)).join('\n');
}
