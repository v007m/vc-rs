import type { Series } from './LineChart.svelte';
import type { HostStats, LoadMetric, LoadPhase, LoadRun, LoadThreshold } from './port.ts';

/** The three live charts (ui.md §6): latency percentiles, throughput and the error rate. */
export type ChartSpec = Readonly<{
  title: string;
  sub: string;
  unit: string;
  series: readonly Series[];
}>;

export const charts: readonly ChartSpec[] = [
  {
    title: 'Latency',
    sub: 'p50, p95 and p99',
    unit: ' ms',
    series: [
      { metric: 'p50', label: 'p50', tone: 'a' },
      { metric: 'p95', label: 'p95', tone: 'b' },
      { metric: 'p99', label: 'p99', tone: 'c' },
    ],
  },
  {
    title: 'Throughput',
    sub: 'requests a second',
    unit: '',
    series: [{ metric: 'rps', label: 'rps', tone: 'a' }],
  },
  {
    title: 'Error rate',
    sub: 'failed iterations',
    unit: ' %',
    series: [{ metric: 'errorRate', label: 'errors', tone: 'c' }],
  },
];

/** The threshold a chart draws: the first one on any metric it shows. */
export function thresholdFor(run: LoadRun, chart: ChartSpec): LoadThreshold | undefined {
  const shown: readonly LoadMetric[] = chart.series.map((s) => s.metric);
  return run.thresholds.find((t) => shown.includes(t.metric));
}

export const phaseLabel: Readonly<Record<LoadPhase, string>> = {
  running: 'Running',
  held: 'Thresholds held',
  breached: 'Threshold breached',
  stopped: 'Stopped',
};

export const phaseClass: Readonly<Record<LoadPhase, string>> = {
  running: 'vc-accent',
  held: 'vc-pass',
  breached: 'vc-fail',
  stopped: 'vc-skipped',
};

/** The KPIs over the charts, from the newest tick. */
export function kpis(run: LoadRun): readonly (readonly [string, string])[] {
  const last = run.ticks.at(-1);
  if (!last) return [];
  return [
    ['Users', String(last.users)],
    ['Requests a second', Math.round(last.rps).toLocaleString('en-US')],
    ['p95', `${Math.round(last.p95)} ms`],
    ['p99', `${Math.round(last.p99)} ms`],
    ['Errors', `${+last.errorRate.toFixed(2)} %`],
  ];
}

/** CPU over this share, or any dropped iteration, means the generator itself is the limit. */
export const HOST_BUSY = 85;

/** Why the numbers may understate the service, when the load generator is short of room. */
export function hostWarning(host: HostStats): string | undefined {
  if (host.dropped > 0)
    return `${host.dropped} iterations were dropped: the generator could not keep up, so the service saw less load than planned.`;
  if (host.cpu >= HOST_BUSY)
    return `The generator is at ${Math.round(host.cpu)} % CPU, so its own delay may add to the latency shown.`;
  return undefined;
}
