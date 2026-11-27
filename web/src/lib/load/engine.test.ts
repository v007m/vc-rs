import { describe, expect, it } from 'vitest';
import { DefinitionKind, LoadEnding } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { engineLoadPort, phaseOf } from './engine.ts';
import { loadNotYet, type LoadUpdate } from './port.ts';

const tick = {
  second: 3n,
  users: 50,
  rps: 200n,
  p50Ms: 40,
  p95Ms: 90,
  p99Ms: 610,
  errors: 4n,
  breaching: ['p99'],
  host: { cpuPercent: 62, memoryBytes: 512n * 1024n * 1024n, lateIterations: 2n },
};

const report = {
  exit: 3,
  scenario: 'checkout.peak',
  ending: LoadEnding.FINISHED,
  seconds: 60n,
  iterations: 12_000n,
  errors: 24n,
  rps: 200,
  p50Ms: 40,
  p90Ms: 80,
  p95Ms: 90,
  p99Ms: 610,
  maxMs: 1200,
  breaches: [
    { metric: 'p99', actual: 610, unit: 'ms', limitJson: '500', shown: '610 ms (lt 500 ms)' },
  ],
  errorKinds: [{ api: 'checkout#pay', status: 502, count: 24n }],
  run: 15n,
  seed: 42n,
};

describe('engineLoadPort', () => {
  it('streams RunLoad as ticks, then the report with its verdict', async () => {
    const port = engineLoadPort(
      fakeEngine({
        async *runLoad(req) {
          expect(req.scenario).toBe('checkout.peak');
          yield { kind: { case: 'tick', value: tick } };
          yield { kind: { case: 'report', value: report } };
        },
      }),
      () => 1000,
    );
    const started = await port.start('checkout.peak', new AbortController().signal);
    if (!started.available) throw new Error(started.reason);
    expect(started.value.run).toMatchObject({
      file: 'checkout.peak',
      startedMs: 1000,
      phase: 'running',
    });
    const seen: LoadUpdate[] = [];
    for await (const update of started.value.updates) seen.push(update);
    expect(seen[0]).toEqual({
      tick: {
        t: 3,
        users: 50,
        rps: 200,
        p50: 40,
        p95: 90,
        p99: 610,
        errorRate: 2,
        breaching: ['p99'],
        host: { cpu: 62, memMb: 512, conns: undefined, late: 2 },
      },
    });
    expect(seen[1]).toMatchObject({
      finished: 'breached',
      id: 15,
      report: { seconds: 60, iterations: 12000, seed: '42', breaches: [{ metric: 'p99' }] },
    });
  });

  it('reads how a load run ended', () => {
    expect(phaseOf({ exit: 0, ending: LoadEnding.FINISHED })).toBe('held');
    expect(phaseOf({ exit: 3, ending: LoadEnding.FINISHED })).toBe('breached');
    expect(phaseOf({ exit: 1, ending: LoadEnding.ABORTED })).toBe('breached');
    expect(phaseOf({ exit: 130, ending: LoadEnding.CANCELLED })).toBe('stopped');
  });

  it('lists the load files and opens a stored load run without ticks', async () => {
    const port = engineLoadPort(
      fakeEngine({
        listDefinitions: (req) => {
          expect(req.kind).toBe(DefinitionKind.FILES);
          return {
            definitions: [
              { id: 'checkout.peak', path: 'services/checkout/peak.load.yaml' },
              { id: 'users.crud', path: 'services/users/crud.test.yaml' },
            ],
          };
        },
        getRun: () => ({
          run: { id: 15n, kind: 'load', selector: 'checkout.peak', status: RunStatus.FAILED },
        }),
      }),
    );
    expect(await port.scenarios()).toEqual({ available: true, value: ['checkout.peak'] });
    const stored = await port.stored(15);
    expect(stored.available && stored.value).toMatchObject({
      id: 15,
      phase: 'breached',
      ticks: [],
    });
  });

  it('says why when the engine has no RunLoad', async () => {
    const port = engineLoadPort(fakeEngine({}));
    expect(await port.start('checkout.peak', new AbortController().signal)).toEqual({
      available: false,
      reason: loadNotYet,
    });
  });
});
