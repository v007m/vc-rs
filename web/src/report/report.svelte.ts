import { SvelteSet } from 'svelte/reactivity';
import type { Status } from '#lib/results/status.ts';
import type { CheckLine, ExchangeLine, TestRow } from '#lib/results/types.ts';
import {
  checkLine,
  exchangeLine,
  numbersOf,
  testRow,
  type IndexRow,
  type Summary,
  type TestData,
} from './data.ts';

/** Where the report's data comes from (a `DataStore`). */
export type Source = Readonly<{ get: (key: string) => Promise<unknown> }>;

/** The open test: its row and its detail. */
export type Opened = Readonly<{
  row: TestRow;
  test: Omit<TestData, 'checks' | 'exchanges'>;
  checks: CheckLine[];
  exchanges: ExchangeLine[];
}>;

/** `#/test/<label>`: the address of an open test. */
export function hashFor(key: string): string {
  return `#/test/${encodeURIComponent(key)}`;
}

function keyOf(hash: string): string | undefined {
  const match = /^#\/test\/(.+)$/.exec(hash);
  if (!match?.[1]) return undefined;
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return undefined;
  }
}

/** The HTML report's state: the run, its tests filtered by text and outcome, and the open test. */
export class Report {
  summary: Summary | undefined = $state.raw();
  rows: readonly TestRow[] = $state.raw([]);
  text = $state('');
  readonly only = new SvelteSet<Status>();
  current: string | undefined = $state();
  opened: Opened | undefined = $state.raw();
  error: string | undefined = $state();
  readonly shown: readonly TestRow[] = $derived(this.#filter());
  readonly #source: Source;
  readonly #go: (hash: string) => void;
  #numbers: ReadonlyMap<string, number> = numbersOf([]);
  #haystacks: readonly string[] = [];

  /** `go` moves the page to an address, so each open test has a link and a Back step. */
  constructor(source: Source, go: (hash: string) => void) {
    this.#source = source;
    this.#go = go;
  }

  async start(): Promise<void> {
    try {
      const [summary, index] = await Promise.all([
        this.#source.get('summary'),
        this.#source.get('index'),
      ]);
      this.#index(summary as Summary, index as readonly IndexRow[]);
      this.summary = summary as Summary;
    } catch (error) {
      this.error = String(error instanceof Error ? error.message : error);
    }
  }

  toggle(status: Status): void {
    if (!this.only.delete(status)) this.only.add(status);
  }

  /** Opens the test at `hash`, unless it is already open. */
  route(hash: string): Promise<void> {
    const key = keyOf(hash);
    return key === undefined || key === this.current ? Promise.resolve() : this.open(key);
  }

  async open(key: string): Promise<void> {
    const number = this.#numbers.get(key);
    const row = number === undefined ? undefined : this.rows[number];
    if (number === undefined || !row || !this.summary) return;
    performance.clearMeasures('vib:open');
    performance.mark('vib:select');
    this.current = key;
    this.#go(hashFor(key));
    try {
      const shard = await this.#source.get(`tests/${Math.floor(number / this.summary.shardSize)}`);
      const test = (shard as Readonly<Record<string, TestData>>)[String(number)];
      if (this.current === key) this.opened = this.#opened(row, test);
    } catch (error) {
      this.error = String(error instanceof Error ? error.message : error);
    }
  }

  #index(summary: Summary, index: readonly IndexRow[]): void {
    this.rows = index.map((row) => testRow(row, summary.files));
    this.#numbers = numbersOf(this.rows);
    this.#haystacks = this.rows.map((row) =>
      [row.key, row.label, row.file, ...row.tags].join(' ').toLowerCase(),
    );
  }

  #filter(): readonly TestRow[] {
    const text = this.text.trim().toLowerCase();
    const only = this.only;
    if (!text && !only.size) return this.rows;
    return this.rows.filter(
      (row, number) =>
        (!only.size || only.has(row.status)) &&
        (!text || (this.#haystacks[number] ?? '').includes(text)),
    );
  }

  #opened(row: TestRow, test: TestData | undefined): Opened {
    const body = async (hash: string): Promise<string> => {
      const text = await this.#source.get(`bodies/${hash}`);
      return typeof text === 'string' ? text : JSON.stringify(text);
    };
    return {
      row,
      test: {
        location: test?.location ?? null,
        blockedBy: test?.blockedBy ?? null,
        skipReason: test?.skipReason ?? null,
        message: test?.message ?? null,
      },
      checks: (test?.checks ?? []).map(checkLine),
      exchanges: (test?.exchanges ?? []).map((exchange) => exchangeLine(exchange, body)),
    };
  }
}
