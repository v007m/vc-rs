import { ConnectError } from '@connectrpc/connect';
import type { HistoryPort, RunLine, RunOutcome, RunScope, StoredRun, TestDetail } from './port.ts';

/** How many recent runs the Runs sidebar lists. */
export const RECENT = 12;

/** How many runs All runs lists. */
export const ALL = 200;

/** All runs' filters that the engine does not apply: a status, and words to look for. */
export type RunFilter = Readonly<{ outcome?: RunOutcome; query?: string }>;

/** Whether `run` has the outcome and every word of `filter.query` (in its number, selector, profile or branch). */
export function matches(run: RunLine, filter: RunFilter): boolean {
  if (filter.outcome && run.outcome !== filter.outcome) return false;
  const text = [`#${run.id}`, run.selector, ...run.profiles, run.branch ?? '', run.commit ?? '']
    .join(' ')
    .toLowerCase();
  const words = (filter.query ?? '').toLowerCase().split(/\s+/).filter(Boolean);
  return words.every((word) => text.includes(word));
}

/** Each value `pick` gives over `runs`, once, in first-seen order (the filter menus' choices). */
export function distinct(
  runs: readonly RunLine[],
  pick: (run: RunLine) => readonly string[],
): string[] {
  return runs.flatMap(pick).filter((v, i, all) => v && all.indexOf(v) === i);
}

/**
 * The Runs space's history (D631): the recent runs for the sidebar, All runs in its scope and
 * filter, and the stored run open with its open test.
 */
export class RunHistory {
  recent: readonly RunLine[] = $state.raw([]);
  runs: readonly RunLine[] = $state.raw([]);
  scope: RunScope = $state.raw({});
  filter: RunFilter = $state.raw({});
  /** Read once, never changed in place. */
  open: StoredRun | undefined = $state.raw();
  current: string | undefined = $state();
  error: string | undefined = $state();
  readonly shown: readonly RunLine[] = $derived(
    this.runs.filter((run) => matches(run, this.filter)),
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
    return failedIds(this.open?.tests ?? []);
  }

  /** Reads the recent runs and All runs again. */
  async load(): Promise<void> {
    const scope = this.scope;
    await this.#read(async () => {
      const [recent, runs] = await Promise.all([
        this.#port.runs(RECENT),
        this.#port.runs(ALL, scope),
      ]);
      this.recent = recent;
      if (this.scope === scope) this.runs = runs;
    });
  }

  /** Lists the runs in `scope`, which the engine narrows; a listing for an older scope is dropped. */
  async rescope(scope: RunScope): Promise<void> {
    this.scope = scope;
    await this.#read(async () => {
      const runs = await this.#port.runs(ALL, scope);
      if (this.scope === scope) this.runs = runs;
    });
  }

  /** Opens run `id`, its first test that did not pass selected. */
  async show(id: number): Promise<void> {
    if (this.open?.run.id === id) return;
    await this.#read(async () => {
      const stored = await this.#port.run(id);
      this.open = stored;
      const first = stored.tests.find(({ row }) => row.status !== 'passed') ?? stored.tests[0];
      this.current = first?.row.key;
    });
  }

  /** The run before `id` in All runs (of the same kind), to compare it with. */
  previous(id: number): RunLine | undefined {
    const at = this.runs.findIndex((run) => run.id === id);
    const kind = this.runs[at]?.kind;
    return at < 0 ? undefined : this.runs.slice(at + 1).find((run) => run.kind === kind);
  }

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

/** The IDs of the tests that failed or were blocked, each once: what **Re-run failed** runs. */
export function failedIds(tests: readonly TestDetail[]): string[] {
  const ids = tests
    .filter(({ row }) => row.status === 'failed' || row.status === 'blocked')
    .map(({ id }) => id);
  return ids.filter((id, i) => ids.indexOf(id) === i);
}
