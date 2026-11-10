import { ConnectError } from '@connectrpc/connect';
import type { InsightsPort, RunSummary, TestTrend } from './port.ts';

/** The time ranges Insights offers, in runs. */
export const windows = [10, 30, 100] as const;
export type Window = (typeof windows)[number];

/** Insights' state: the newest runs in the window and each test's trend over them. */
export class Insights {
  window: Window = $state(30);
  /** Oldest first, as the charts read left to right. */
  runs: readonly RunSummary[] = $state([]);
  trends: readonly TestTrend[] = $state([]);
  loading = $state(false);
  error: string | undefined = $state();
  readonly #port: InsightsPort;

  constructor(port: InsightsPort) {
    this.#port = port;
  }

  async load(): Promise<void> {
    this.loading = true;
    try {
      const [runs, trends] = await Promise.all([
        this.#port.runs(this.window),
        this.#port.trends(this.window),
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
