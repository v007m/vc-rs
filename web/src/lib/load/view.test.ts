import { describe, expect, it } from 'vitest';
import type { HostStats, LoadRun, LoadThreshold, LoadTick } from './port.ts';
import { charts, hostWarning, kpis, thresholdFor } from './view.ts';

const HOST: HostStats = { cpu: 40, memMb: 512, conns: 80, late: 0 };

const TICK: LoadTick = {
  t: 30,
  users: 50,
  rps: 1234.6,
  p50: 80,
  p95: 210.4,
  p99: 349.6,
  errorRate: 0.1234,
  breaching: [],
  host: HOST,
};

const limit = (metric: LoadThreshold['metric'], value: number): LoadThreshold => ({
  metric,
  limit: value,
  label: `${metric} < ${value}`,
  abortOn: false,
});

function run(thresholds: LoadThreshold[], ticks: LoadTick[] = []): LoadRun {
  return {
    file: 'checkout.load',
    profile: 'staging',
    startedMs: 0,
    plannedS: 120,
    phase: 'running',
    thresholds,
    ticks,
  };
}

describe('thresholdFor', () => {
  it('draws the first threshold on a metric the chart shows', () => {
    const [latency, throughput, errors] = charts;
    const shown = run([limit('errorRate', 1), limit('p99', 500), limit('p95', 300)]);
    expect(latency && thresholdFor(shown, latency)?.metric).toBe('p99');
    expect(throughput && thresholdFor(shown, throughput)).toBeUndefined();
    expect(errors && thresholdFor(shown, errors)?.metric).toBe('errorRate');
  });
});

describe('kpis', () => {
  it('shows no KPI before the first tick', () => {
    expect(kpis(run([]))).toEqual([]);
  });

  it('reads the newest tick, rounded for the eye', () => {
    expect(kpis(run([], [{ ...TICK, users: 10 }, TICK]))).toEqual([
      ['Users', '50'],
      ['Requests a second', '1,235'],
      ['p95', '210 ms'],
      ['p99', '350 ms'],
      ['Errors', '0.12 %'],
    ]);
  });
});

describe('hostWarning', () => {
  it('warns when iterations started late, before a busy CPU', () => {
    expect(hostWarning({ ...HOST, cpu: 95, late: 3 })).toBe(
      '3 iterations started late: the generator could not keep up, so the service saw less load than planned.',
    );
  });

  it('warns when the generator CPU is at 85 % or more', () => {
    expect(hostWarning({ ...HOST, cpu: 85.4 })).toBe(
      'The generator is at 85 % CPU, so its own delay may add to the latency shown.',
    );
  });

  it('says nothing while the generator has room', () => {
    expect(hostWarning(HOST)).toBeUndefined();
    expect(hostWarning({ late: 0 })).toBeUndefined();
  });
});
