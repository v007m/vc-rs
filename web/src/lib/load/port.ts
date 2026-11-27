import { notYet, type Maybe } from '#lib/api/maybe.ts';

/**
 * The load generator's own state at one tick (`LoadTick.host`, D674–D677): CPU in percent of one
 * core, resident memory, open connections, each absent when the platform does not tell it, and
 * the iterations so far that started late.
 */
export type HostStats = Readonly<{ cpu?: number; memMb?: number; conns?: number; late: number }>;

/** One tick of a load run: seconds from its start and what it measured over the window. */
export type LoadTick = Readonly<{
  t: number;
  users: number;
  rps: number;
  p50: number;
  p95: number;
  p99: number;
  /** In percent. */
  errorRate: number;
  /** The thresholds failing over the window, such as `p95` or `<test>: errorRate`. */
  breaching: readonly string[];
  host: HostStats;
}>;

/** A metric a threshold can hold a limit on. */
export type LoadMetric = 'p50' | 'p95' | 'p99' | 'errorRate' | 'rps';

/** One `thresholds:` entry: the metric stays under `limit`; `abortOn` stops the run on a breach. */
export type LoadThreshold = Readonly<{
  metric: LoadMetric;
  limit: number;
  label: string;
  abortOn: boolean;
}>;

/** One threshold the whole run breached, as the summary shows it: `45.5 ms (lt 30 ms)`. */
export type LoadBreach = Readonly<{ metric: string; test?: string; shown: string }>;

/** Latencies in ms, from a whole run or one mix test. */
export type Latency = Readonly<{ p50: number; p90: number; p95: number; p99: number; max: number }>;

/** One mix test's iterations over the whole run. */
export type LoadTestLine = Readonly<{
  test: string;
  iterations: number;
  errors: number;
  rps: number;
  latency: Latency;
}>;

/** Failed iterations of one kind: the api, test, last status and code they share. */
export type LoadErrorLine = Readonly<{
  api?: string;
  test?: string;
  status?: number;
  code?: string;
  count: number;
}>;

/** The report of a finished load run, as `vc load --json` gives it (D678). */
export type LoadReport = Readonly<{
  seconds: number;
  iterations: number;
  errors: number;
  rps: number;
  latency: Latency;
  breaches: readonly LoadBreach[];
  tests: readonly LoadTestLine[];
  errorKinds: readonly LoadErrorLine[];
  /** The shared action's failure that kept the users from starting. */
  blocked?: string;
  seed: string;
  notices: readonly string[];
}>;

/** Where a load run is. */
export type LoadPhase = 'running' | 'held' | 'breached' | 'stopped';

/** One load run, live or from the history. */
export type LoadRun = Readonly<{
  id?: number;
  /** The scenario's ID: its load file's ID, such as `checkout.peak`. */
  file: string;
  profile: string;
  startedMs: number;
  /** Its stages' whole length in seconds, when known; the axis grows with the ticks without. */
  plannedS: number;
  phase: LoadPhase;
  thresholds: readonly LoadThreshold[];
  ticks: readonly LoadTick[];
  report?: LoadReport;
}>;

/** What a running load run sends: a tick, or how it ended with its report. */
export type LoadUpdate =
  | Readonly<{ tick: LoadTick }>
  | Readonly<{ finished: Exclude<LoadPhase, 'running'>; id?: number; report: LoadReport }>;

/** The load runs' RPCs (ui.md §10 row 8): `RunLoad`, a stream, and a stored load run. */
export interface LoadPort {
  /** Starts `file`; the run's opening state and its updates. */
  start(
    file: string,
    signal: AbortSignal,
  ): Promise<Maybe<Readonly<{ run: LoadRun; updates: AsyncIterable<LoadUpdate> }>>>;
  stored(id: number): Promise<Maybe<LoadRun>>;
  /** The project's load scenarios by ID, to start one. */
  scenarios(): Promise<Maybe<readonly string[]>>;
}

export const loadNotYet = 'Load runs need a newer engine.';

/** An engine older than row 8: no load runs. */
export const noLoadPort: LoadPort = {
  start: () => Promise.resolve(notYet(loadNotYet)),
  stored: () => Promise.resolve(notYet(loadNotYet)),
  scenarios: () => Promise.resolve(notYet(loadNotYet)),
};
