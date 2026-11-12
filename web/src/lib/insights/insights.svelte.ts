import { ConnectError } from '@connectrpc/connect';
import type { InsightsPort, RunSummary, Scope, SkipLine, TestTrend } from './port.ts';
import { localDay } from './stats.ts';

/** The time ranges Insights offers, in runs. */
export const windows = [10, 30, 100] as const;
export type Window = (typeof windows)[number];

/** Insights' state: the newest runs in the window and scope, and each test's trend over them. */
export class Insights {
  window: Window = $state(30);
  /** Oldest first, as the charts read left to right. */
  runs: readonly RunSummary[] = $state([]);
  trends: readonly TestTrend[] = $state([]);
  /** The skips that have an end date, expired ones included. */
  skips: readonly SkipLine[] = $state([]);
  loading = $state(false);
  error: string | undefined = $state();
  /** The local day the skips' dates are read against, `YYYY-MM-DD`. */
  readonly today: string;
  readonly #port: InsightsPort;

  constructor(port: InsightsPort, today = localDay(Date.now())) {
    this.#port = port;
    this.today = today;
  }

  /** Reads the runs and the skips. */
  async open(): Promise<void> {
    await Promise.all([this.load(), this.loadSkips()]);
  }

  /** Reads the skips again, after one is written or removed. */
  async loadSkips(): Promise<void> {
    try {
      this.skips = await this.#port.skips();
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }

  async load(): Promise<void> {
    this.loading = true;
    const scope: Scope = {};
    try {
      const [runs, trends] = await Promise.all([
        this.#port.runs(this.window, scope),
        this.#port.trends(this.window, scope),
      ]);
      this.runs = [...runs].reverse();
      this.trends = trends;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.loading = false;
    }
  }

  async choose(window: Window): Promise<void> {
    if (window === this.window) return;
    this.window = window;
    await this.load();
  }
}
