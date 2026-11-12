import { ConnectError } from '@connectrpc/connect';
import type { HistoryPort, RunLine, StoredRun, TestDetail } from './port.ts';

/** How many recent runs the Runs screen lists. */
export const RECENT = 20;

/** The Runs screen's history: the recent runs, the stored run open, and its open test. */
export class RunHistory {
  runs: readonly RunLine[] = $state([]);
  /** Read once, never changed in place. */
  open: StoredRun | undefined = $state.raw();
  current: string | undefined = $state();
  error: string | undefined = $state();
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
    });
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
