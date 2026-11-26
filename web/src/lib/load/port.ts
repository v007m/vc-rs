import { notYet, type Maybe } from '#lib/api/maybe.ts';

/** The load generator's own state at one tick (`load_tick`, ui.md §6). */
export type HostStats = Readonly<{ cpu: number; memMb: number; conns: number; dropped: number }>;

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

/** One threshold's verdict in the report. */
export type ThresholdVerdict = Readonly<{
  threshold: LoadThreshold;
  scope: string;
  held: boolean;
  worst: number;
  /** Seconds from the start of the worst window. */
  at: number;
  whole: number;
}>;

/** One test of the scenario mix. */
export type ScenarioLine = Readonly<{
  test: string;
  weight: number;
  iterations: number;
  p50: number;
  p95: number;
  p99: number;
  errorRate: number;
  held: boolean;
}>;

/** The report of a finished load run (ui.md §6). */
export type LoadReport = Readonly<{
  verdicts: readonly ThresholdVerdict[];
  /** Iterations per latency bucket, by its upper bound in ms. */
  histogram: readonly (readonly [number, number])[];
  scenarios: readonly ScenarioLine[];
  /** p99 of earlier runs of the same file, newest first, this run first. */
  previous: readonly Readonly<{ run: number; p99: number }>[];
}>;

/** Where a load run is. */
export type LoadPhase = 'running' | 'held' | 'breached' | 'stopped';

/** One load run, live or from the history. */
export type LoadRun = Readonly<{
  id?: number;
  file: string;
  profile: string;
  startedMs: number;
  /** Its stages' whole length, in seconds. */
  plannedS: number;
  phase: LoadPhase;
  thresholds: readonly LoadThreshold[];
  ticks: readonly LoadTick[];
  report?: LoadReport;
}>;

/** What a running load run sends: a tick, or how it ended with its report. */
export type LoadUpdate =
  | Readonly<{ tick: LoadTick }>
  | Readonly<{ finished: Exclude<LoadPhase, 'running'>; id: number; report: LoadReport }>;

/** The load runs' RPCs (ui.md §10 row 8): `RunLoad`, a stream, and a stored load run. */
export interface LoadPort {
  /** Starts `file`; the run's opening state and its updates. */
  start(
    file: string,
    signal: AbortSignal,
  ): Promise<Maybe<Readonly<{ run: LoadRun; updates: AsyncIterable<LoadUpdate> }>>>;
  stored(id: number): Promise<Maybe<LoadRun>>;
}

export const loadNotYet = 'Load runs need a newer engine.';

/** Today's engine: no load runs. */
export const noLoadPort: LoadPort = {
  start: () => Promise.resolve(notYet(loadNotYet)),
  stored: () => Promise.resolve(notYet(loadNotYet)),
};
