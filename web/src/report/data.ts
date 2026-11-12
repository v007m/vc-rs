import { statuses, type Counts, type Status } from '#lib/results/status.ts';
import type { CheckLine, ExchangeLine, LazyBody, TestRow } from '#lib/results/types.ts';

// The report's data files, as `vc-adapter-reporters` writes them (html_data.rs). They are
// untrusted text: the report renders every value as text, never as markup.

export type Summary = Readonly<{
  run: Readonly<{
    id: number;
    status: string;
    startedMs: number;
    durationMs: number;
    profiles: readonly string[];
    seed: number | null;
  }>;
  totals: Partial<Counts>;
  lifecycle: readonly Readonly<{ label: string; status: string; message: string | null }>[];
  /** The teardown blocks that failed (D304): `afterAll` with its file, or `afterRun`. */
  teardownFailures: readonly Readonly<{ block: string; file: string | null; message: string }>[];
  files: readonly string[];
  /** Tests per `tests/<shard>` file. */
  shardSize: number;
}>;

/** `[label, name, file index, status, ms, tags]`; the label is unique per test instance. */
export type IndexRow = readonly [
  string,
  string | null,
  number | null,
  string,
  number | null,
  readonly string[],
];

type CheckData = Readonly<{
  label: string;
  passed: boolean;
  expected: string;
  actual: string | null;
  location: string | null;
}>;

type ExchangeData = Readonly<{
  method: string;
  url: string;
  status: number | null;
  ms: number | null;
  requestHeaders: readonly (readonly [string, string])[];
  responseHeaders: readonly (readonly [string, string])[];
  requestBody: string | null;
  responseBody: string | null;
  truncated: boolean;
}>;

/** One test's entry in its `tests/<shard>` file. */
export type TestData = Readonly<{
  location: string | null;
  blockedBy: string | null;
  skipReason: string | null;
  message: string | null;
  checks: readonly CheckData[];
  exchanges: readonly ExchangeData[];
}>;

const known = new Set<string>(statuses);

/** A row's outcome; anything unknown reads as skipped. */
function status(text: string): Status {
  return known.has(text) ? (text as Status) : 'skipped';
}

/** The list row of index row `row`. */
export function testRow(row: IndexRow, files: readonly string[]): TestRow {
  const [label, name, file, outcome, ms, tags] = row;
  return {
    key: label,
    label: name ?? label,
    file: file === null ? '' : (files[file] ?? ''),
    status: status(outcome),
    ms: ms ?? undefined,
    tags,
  };
}

/** Each row's number in the index, by its key. */
export function numbersOf(rows: readonly TestRow[]): ReadonlyMap<string, number> {
  return new Map(rows.map((row, number) => [row.key, number]));
}

export function checkLine(check: CheckData): CheckLine {
  return {
    label: check.label,
    passed: check.passed,
    expected: check.expected,
    actual: check.actual ?? undefined,
    location: check.location ?? undefined,
  };
}

/** The exchange with its bodies fetched by `body` on demand. */
export function exchangeLine(
  exchange: ExchangeData,
  body: (hash: string) => Promise<string>,
): ExchangeLine {
  const lazy = (hash: string | null): LazyBody | undefined =>
    hash === null ? undefined : () => body(hash);
  return {
    method: exchange.method,
    url: exchange.url,
    status: exchange.status ?? undefined,
    ms: exchange.ms ?? undefined,
    requestHeaders: exchange.requestHeaders,
    responseHeaders: exchange.responseHeaders,
    requestBody: lazy(exchange.requestBody),
    responseBody: lazy(exchange.responseBody),
    truncated: exchange.truncated,
  };
}

/** Totals with every outcome present. */
export function counts(totals: Partial<Counts>): Counts {
  return {
    passed: totals.passed ?? 0,
    failed: totals.failed ?? 0,
    blocked: totals.blocked ?? 0,
    flaky: totals.flaky ?? 0,
    skipped: totals.skipped ?? 0,
  };
}

/** `teardown failed: afterAll in users.crud, afterRun`, as the console says it; empty when none failed. */
export function teardownLine(failures: Summary['teardownFailures']): string {
  const blocks = failures.map((failed) => (failed.file ? `${failed.block} in ${failed.file}` : failed.block));
  return blocks.length ? `teardown failed: ${blocks.join(', ')}` : '';
}
