import type { Status } from './status.ts';

/** One test in a list of results: `key` is unique in the list (a test instance's label). */
export type TestRow = Readonly<{
  key: string;
  label: string;
  file: string;
  status: Status;
  ms?: number;
  tags: readonly string[];
}>;

/** One evaluated check: the expected and actual values are already rendered as text. */
export type CheckLine = Readonly<{
  label: string;
  passed: boolean;
  expected: string;
  actual?: string;
  /** `path:line` of the check in the source. */
  location?: string;
}>;

/** A body that is fetched only when someone looks at it. */
export type LazyBody = () => Promise<string>;

/** One request and its response. */
export type ExchangeLine = Readonly<{
  method: string;
  url: string;
  /** Absent when no response came back. */
  status?: number;
  ms?: number;
  requestHeaders: readonly (readonly [string, string])[];
  responseHeaders: readonly (readonly [string, string])[];
  requestBody?: LazyBody;
  responseBody?: LazyBody;
  /** Whether the bodies were cut at `history.maxBodySize`. */
  truncated: boolean;
}>;

/** JSON pretty-printed, or the text as it is. */
export function pretty(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}
