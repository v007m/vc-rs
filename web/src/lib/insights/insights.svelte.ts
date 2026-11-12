import { ConnectError } from '@connectrpc/connect';
import type {
  ApiTrend,
  Coverage,
  InsightsPort,
  RunSummary,
  Scope,
  SkipLine,
  TestTrend,
} from './port.ts';
import { localDay } from './stats.ts';

/** The time ranges Insights offers, in runs. */
export const windows = [10, 30, 100] as const;
export type Window = (typeof windows)[number];

/** Insights' state: the newest runs in the window and scope, and each test's and api's trend over them. */
export class Insights {
  window: Window = $state(30);
  /** Oldest first, as the charts read left to right. */
  runs: readonly RunSummary[] = $state([]);
  trends: readonly TestTrend[] = $state([]);
  apiTrends: readonly ApiTrend[] = $state([]);
  /** The skips that have an end date, expired ones included. */
  skips: readonly SkipLine[] = $state([]);
  /** What tests and documents each api, whatever the window. */
  coverage: Coverage = $state({ apis: [], notes: [] });
  loading = $state(false);
  error: string | undefined = $state();
  /** The local day the skips' dates are read against, `YYYY-MM-DD`. */
  readonly today: string;
  readonly #port: InsightsPort;

  constructor(port: InsightsPort, today = localDay(Date.now())) {
    this.#port = port;
    this.today = today;
  }

  /** Reads the runs, the skips and the coverage. */
  async open(): Promise<void> {
    await Promise.all([this.load(), this.loadSkips(), this.loadCoverage()]);
  }

  async loadCoverage(): Promise<void> {
    try {
      this.coverage = await this.#port.coverage();
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
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
      const [runs, trends, apiTrends] = await Promise.all([
        this.#port.runs(this.window, scope),
        this.#port.trends(this.window, scope),
        this.#port.apiTrends(this.window, scope),
      ]);
      this.runs = [...runs].reverse();
      this.trends = trends;
      this.apiTrends = apiTrends;
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
