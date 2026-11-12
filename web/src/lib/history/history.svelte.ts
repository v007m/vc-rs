import { ConnectError } from '@connectrpc/connect';
import type { HistoryPort, RunLine, StoredRun, TestDetail } from './port.ts';

/** How many recent runs the Runs screen lists. */
export const RECENT = 20;

/** A selector the recent runs used, and how many of them used it. */
export type SelectorUse = Readonly<{ selector: string; runs: number }>;

/** Each selector `runs` used, newest first, with how many used it; `''` is all tests. */
export function selectorsOf(runs: readonly RunLine[]): SelectorUse[] {
  const used = runs.map((run) => run.selector);
  const selectors = used.filter((selector, at) => used.indexOf(selector) === at);
  return selectors.map((selector) => ({
    selector,
    runs: used.filter((other) => other === selector).length,
  }));
}

/** The Runs screen's history: the recent runs, the stored run open, and its open test. */
export class RunHistory {
  runs: readonly RunLine[] = $state([]);
  /** Read once, never changed in place. */
  open: StoredRun | undefined = $state.raw();
  current: string | undefined = $state();
  error: string | undefined = $state();
  /** The selector the recent runs are filtered by; none lists them all. */
  selector: string | undefined = $state();
  readonly selectors: readonly SelectorUse[] = $derived(selectorsOf(this.runs));
  readonly shown: readonly RunLine[] = $derived(
    this.runs.filter((run) => this.selector === undefined || run.selector === this.selector),
  );
  readonly opened: TestDetail | undefined = $derived(
    this.open?.tests.find((test) => test.row.key === this.current),
  );
  readonly #port: HistoryPort;

  constructor(port: HistoryPort) {
    this.#port = port;
  }

  /** The test IDs that failed or were blocked in the open run, each once. */
  get failedIds(): string[] {
    const ids = (this.open?.tests ?? [])
      .filter(({ row }) => row.status === 'failed' || row.status === 'blocked')
      .map(({ id }) => id);
    return ids.filter((id, at) => ids.indexOf(id) === at);
  }

  async load(): Promise<void> {
    await this.#read(async () => {
      this.runs = await this.#port.runs(RECENT);
      if (!this.runs.some((run) => run.selector === this.selector)) this.selector = undefined;
    });
  }

  /** Lists only the runs that used `selector`, or all of them again when it is the one in use. */
  filter(selector: string): void {
    this.selector = this.selector === selector ? undefined : selector;
  }

  /** Opens run `id`, its first test that did not pass selected. */
  async show(id: number): Promise<void> {
    await this.#read(async () => {
      const stored = await this.#port.run(id);
      this.open = stored;
      const first = stored.tests.find(({ row }) => row.status !== 'passed') ?? stored.tests[0];
      this.current = first?.row.key;
    });
  }

  /** Back to the live run. */
  close(): void {
    this.open = undefined;
    this.current = undefined;
  }

  async #read(read: () => Promise<void>): Promise<void> {
    try {
      await read();
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }
}
