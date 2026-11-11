import { ConnectError } from '@connectrpc/connect';
import type { InsightsPort, RunSummary, Scope, SkipLine, TestTrend } from './port.ts';
import { localDay } from './stats.ts';

/** The time ranges Insights offers, in runs. */
export const windows = [10, 30, 100] as const;
export type Window = (typeof windows)[number];

/** Insights' state: the newest runs in the window and scope, and each test's trend over them. */
export class Insights {
  window: Window = $state(30);
  /** Every profile when absent. */
  profile: string | undefined = $state();
  /** Every branch when absent. */
  branch: string | undefined = $state();
  /** The profiles the config defines. */
  profiles: readonly string[] = $state([]);
  /** Every branch a loaded run was on, first seen first: the history keeps no other list. */
  branches: readonly string[] = $state([]);
  /** Oldest first, as the charts read left to right. */
  runs: readonly RunSummary[] = $state([]);
  trends: readonly TestTrend[] = $state([]);
  /** The skipped tests that have an end date. */
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

  /** Reads the profiles and the skips, then the runs. */
  async open(): Promise<void> {
    try {
      this.profiles = await this.#port.profiles();
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
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
    const scope: Scope = { profile: this.profile, branch: this.branch };
    try {
      const [runs, trends] = await Promise.all([
        this.#port.runs(this.window, scope),
        this.#port.trends(this.window, scope),
      ]);
      this.runs = [...runs].reverse();
      this.trends = trends;
      this.#seen(runs);
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

  /** Narrows to one profile or branch, or widens to every one with `undefined`. */
  async scope(key: keyof Scope, value: string | undefined): Promise<void> {
    if (this[key] === value) return;
    this[key] = value;
    await this.load();
  }

  #seen(runs: readonly RunSummary[]): void {
    const found = runs.flatMap((run) => (run.branch ? [run.branch] : []));
    const fresh = found.filter((b, at) => !this.branches.includes(b) && found.indexOf(b) === at);
    if (fresh.length) this.branches = [...this.branches, ...fresh];
  }
}
