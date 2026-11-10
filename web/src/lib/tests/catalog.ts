import type { Definition } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** One test as the Tests screen lists it. */
export type TestRow = Readonly<{
  /** The full ID, `<fileId>#<id>`. */
  id: string;
  /** The ID within its file. */
  local: string;
  name: string;
  fileId: string;
  /** The first word of the file ID, which is its service folder. */
  service: string;
  path: string;
  tags: readonly string[];
  skipped: boolean;
}>;

/** A test file and the tests in it, in file order. */
export type FileGroup = Readonly<{ fileId: string; path: string; tests: readonly TestRow[] }>;

/** A test definition as a row. */
export function toRow(definition: Definition): TestRow {
  const [fileId = '', local = definition.id] = definition.id.split('#');
  return {
    id: definition.id,
    local,
    name: definition.name ?? local,
    fileId,
    service: fileId.split('.')[0] ?? fileId,
    path: definition.path,
    tags: definition.tags,
    skipped: definition.skipped,
  };
}

/** A parsed filter query: bare words plus `key:value` filters, all combined with AND. */
export type Query = Readonly<{
  words: readonly string[];
  filters: readonly Readonly<{ key: FilterKey; value: string }>[];
}>;

type Test = (row: TestRow, value: string) => boolean;

/** What each filter key matches; a key not listed here is read as a bare word. */
const filterTests = {
  tag: (row, value) => row.tags.includes(value),
  service: (row, value) => row.service === value,
  file: (row, value) => row.fileId === value || row.fileId.startsWith(`${value}.`),
  is: (row, value) => (value === 'skipped' ? row.skipped : value === 'active' && !row.skipped),
} satisfies Record<string, Test>;

export type FilterKey = keyof typeof filterTests;

/** The filter keys, for the hints under the query box. */
export const filterKeys = Object.keys(filterTests) as FilterKey[];

function isFilterKey(key: string): key is FilterKey {
  return Object.hasOwn(filterTests, key);
}

const token = /(\w+):"([^"]*)"|(\w+):(\S+)|"([^"]*)"|(\S+)/g;

/** Parses `text`; values with spaces are quoted, as in `tag:"slow path"`. */
export function parseQuery(text: string): Query {
  const words: string[] = [];
  const filters: { key: FilterKey; value: string }[] = [];
  for (const [whole, qKey, qValue, key, value, quoted, bare] of text.matchAll(token)) {
    const k = qKey ?? key;
    const v = qValue ?? value;
    if (k !== undefined && v !== undefined && isFilterKey(k)) filters.push({ key: k, value: v });
    else words.push((quoted ?? bare ?? whole).toLowerCase());
  }
  return { words, filters };
}

/** Whether `row` passes every filter and contains every word in its id, name or file. */
export function matches(row: TestRow, query: Query): boolean {
  const haystack = `${row.id} ${row.name} ${row.path}`.toLowerCase();
  return (
    query.words.every((word) => haystack.includes(word)) &&
    query.filters.every(({ key, value }) => filterTests[key](row, value))
  );
}

/** `rows` grouped by file, files in first-seen order. */
export function groupByFile(rows: readonly TestRow[]): FileGroup[] {
  const groups = new Map<string, { fileId: string; path: string; tests: TestRow[] }>();
  for (const row of rows) {
    const group = groups.get(row.fileId) ?? { fileId: row.fileId, path: row.path, tests: [] };
    group.tests.push(row);
    groups.set(row.fileId, group);
  }
  return [...groups.values()];
}

/** A facet value and how many rows have it. */
export type Facet = Readonly<{ value: string; count: number }>;

/** How many rows have each value `pick` returns, most common first, then by name. */
export function facets(
  rows: readonly TestRow[],
  pick: (row: TestRow) => readonly string[],
): Facet[] {
  const counts = new Map<string, number>();
  for (const row of rows)
    for (const value of pick(row)) counts.set(value, (counts.get(value) ?? 0) + 1);
  return [...counts]
    .map(([value, count]) => ({ value, count }))
    .sort((a, b) => b.count - a.count || a.value.localeCompare(b.value));
}

/** `text` with `key:value` added, or removed when it is already there. */
export function toggleFilter(text: string, key: FilterKey, value: string): string {
  const term = /\s/.test(value) ? `${key}:"${value}"` : `${key}:${value}`;
  const parts = text.split(/\s+/).filter(Boolean);
  return parts.includes(term)
    ? parts.filter((p) => p !== term).join(' ')
    : [...parts, term].join(' ');
}
