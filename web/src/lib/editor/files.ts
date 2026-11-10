import { Severity, type Diagnostic } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** A folder of the project and the files directly in it. */
export type Folder = Readonly<{ dir: string; files: readonly string[] }>;

/** The folder part of a path, `''` at the project root. */
export function dirName(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash < 0 ? '' : path.slice(0, slash);
}

/** The file name part of a path. */
export function baseName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

/** A folder's tree key; it ends in `/`, which a file path never does. */
export function folderKey(dir: string): string {
  return `${dir}/`;
}

/** The files whose path contains `filter`, grouped by folder, folders and files sorted. */
export function folders(paths: readonly string[], filter: string): Folder[] {
  const needle = filter.trim().toLowerCase();
  const byDir = new Map<string, string[]>();
  for (const path of [...paths].sort()) {
    if (needle && !path.toLowerCase().includes(needle)) continue;
    const dir = dirName(path);
    byDir.set(dir, [...(byDir.get(dir) ?? []), path]);
  }
  return [...byDir].sort(([a], [b]) => a.localeCompare(b)).map(([dir, files]) => ({ dir, files }));
}

/** Errors first, then by line; diagnostics without a line go last. */
export function byImportance(diagnostics: readonly Diagnostic[]): Diagnostic[] {
  const line = (d: Diagnostic) => d.primary?.span?.startLine || Number.MAX_SAFE_INTEGER;
  const error = (d: Diagnostic) => (d.severity === Severity.ERROR ? 0 : 1);
  return [...diagnostics].sort((a, b) => error(a) - error(b) || line(a) - line(b));
}

/** Whether a run can start from this file: test and load files hold runnable tests. */
export function runnable(path: string): boolean {
  return /\.(test|load)\.ya?ml$/.test(path);
}
