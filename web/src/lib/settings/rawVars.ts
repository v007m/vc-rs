/**
 * The vars a config's top-level `vars:` block map sets, as written (`${env.X}` stays as it
 * is), for the plain, single- and double-quoted scalars on one line each. Anything else, such
 * as a flow map, a block scalar or an escape other than `\"` and `\\`, is left out, so the
 * form shows the explained value with a badge instead of guessing.
 */
export function rawVars(text: string): Map<string, string> {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => /^vars:\s*(#.*)?$/.test(line));
  const found = new Map<string, string>();
  let indent: string | undefined;
  for (const line of start < 0 ? [] : lines.slice(start + 1)) {
    if (/^\s*(#.*)?$/.test(line)) continue;
    if (/^\S/.test(line)) break;
    indent ??= /^ +/.exec(line)?.[0];
    const entry = /^( +)([A-Za-z_][A-Za-z0-9_]*):[ ]+(\S.*?)\s*$/.exec(line);
    const value = entry?.[1] === indent ? scalarText(entry?.[3] ?? '') : undefined;
    if (entry?.[2] !== undefined && value !== undefined) found.set(entry[2], value);
  }
  return found;
}

/** The text a one-line scalar holds, or nothing when it is not a plain or quoted scalar. */
export function scalarText(written: string): string | undefined {
  const single = /^'((?:[^']|'')*)'\s*(#.*)?$/.exec(written);
  if (single) return single[1]?.replaceAll("''", "'");
  const double = /^"((?:[^"\\]|\\["\\])*)"\s*(#.*)?$/.exec(written);
  if (double) return double[1]?.replace(/\\(["\\])/g, '$1');
  if (/^["'[{|>&*!%@`]/.test(written)) return undefined;
  return written.replace(/\s+#.*$/, '').trim();
}
