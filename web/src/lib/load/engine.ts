import type { Client } from '@connectrpc/connect';
import { available, unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import {
  DefinitionKind,
  LoadEnding,
  type EngineService,
  type LoadReport as WireReport,
  type RunLoadResponse,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import type { LoadTick as WireTick } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { RunStatus, type Run } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import {
  loadNotYet,
  type LoadPhase,
  type LoadPort,
  type LoadReport,
  type LoadRun,
  type LoadTick,
  type LoadUpdate,
} from './port.ts';

export type LoadClient = Pick<
  Client<typeof EngineService>,
  'runLoad' | 'getRun' | 'listDefinitions'
>;

const MB = 1024 * 1024;

/** One wire tick as the charts read it; errors become a share of the second's iterations. */
export function tickOf(tick: WireTick): LoadTick {
  const rps = Number(tick.rps);
  const host = tick.host;
  return {
    t: Number(tick.second),
    users: tick.users,
    rps,
    p50: tick.p50Ms,
    p95: tick.p95Ms,
    p99: tick.p99Ms,
    errorRate: rps > 0 ? (Number(tick.errors) / rps) * 100 : 0,
    breaching: tick.breaching,
    host: {
      cpu: host?.cpuPercent,
      memMb: host?.memoryBytes === undefined ? undefined : Number(host.memoryBytes) / MB,
      conns: host?.connections === undefined ? undefined : Number(host.connections),
      late: Number(host?.lateIterations ?? 0n),
    },
  };
}

/** How a load run ended: held, breached (exit 3 or aborted), or stopped (cancelled, blocked). */
export function phaseOf(
  report: Pick<WireReport, 'exit' | 'ending'>,
): Exclude<LoadPhase, 'running'> {
  if (report.exit === 3 || report.ending === LoadEnding.ABORTED) return 'breached';
  if (report.ending === LoadEnding.FINISHED && report.exit === 0) return 'held';
  return 'stopped';
}

const latency = (l: {
  p50Ms: number;
  p90Ms: number;
  p95Ms: number;
  p99Ms: number;
  maxMs: number;
}) => ({
  p50: l.p50Ms,
  p90: l.p90Ms,
  p95: l.p95Ms,
  p99: l.p99Ms,
  max: l.maxMs,
});

/** The wire report as the Report view reads it. */
export function reportOf(report: WireReport): LoadReport {
  return {
    seconds: Number(report.seconds),
    iterations: Number(report.iterations),
    errors: Number(report.errors),
    rps: report.rps,
    latency: latency(report),
    breaches: report.breaches.map(({ metric, test, shown }) => ({ metric, test, shown })),
    tests: report.tests.map((t) => ({
      test: t.test,
      iterations: Number(t.iterations),
      errors: Number(t.errors),
      rps: t.rps,
      latency: latency(t),
    })),
    errorKinds: report.errorKinds.map((k) => ({ ...k, count: Number(k.count) })),
    blocked: report.blocked,
    seed: String(report.seed),
    notices: report.notices,
  };
}

function updateOf(response: RunLoadResponse): LoadUpdate | undefined {
  const { kind } = response;
  if (kind.case === 'tick') return { tick: tickOf(kind.value) };
  if (kind.case !== 'report') return undefined;
  const id = kind.value.run === undefined ? undefined : Number(kind.value.run);
  return { finished: phaseOf(kind.value), id, report: reportOf(kind.value) };
}

async function* updates(
  first: IteratorResult<RunLoadResponse>,
  rest: AsyncIterator<RunLoadResponse>,
): AsyncGenerator<LoadUpdate> {
  for (let next = first; !next.done; next = await rest.next()) {
    const update = updateOf(next.value);
    if (update) yield update;
  }
}

const storedPhase: Readonly<Record<RunStatus, LoadPhase>> = {
  [RunStatus.UNSPECIFIED]: 'stopped',
  [RunStatus.PASSED]: 'held',
  [RunStatus.FAILED]: 'breached',
  [RunStatus.CANCELLED]: 'stopped',
};

/** A stored load run: the history keeps its verdict, not its ticks (D677). */
function storedRun(run: Run): LoadRun {
  return {
    id: Number(run.id),
    file: run.selector,
    profile: run.profiles.join(', '),
    startedMs: Number(run.startedMs),
    plannedS: 0,
    phase: storedPhase[run.status] ?? 'stopped',
    thresholds: [],
    ticks: [],
  };
}

/** The load port over `RunLoad` (D678), `GetRun` and the project's load files. */
export function engineLoadPort(client: LoadClient, now: () => number = Date.now): LoadPort {
  return {
    start: (file, signal) =>
      unlessMissing(loadNotYet, async () => {
        const stream = client.runLoad({ scenario: file }, { signal })[Symbol.asyncIterator]();
        const first = await stream.next();
        const run: LoadRun = {
          file,
          profile: '',
          startedMs: now(),
          plannedS: 0,
          phase: 'running',
          thresholds: [],
          ticks: [],
        };
        return { run, updates: updates(first, stream) };
      }),
    stored: async (id): Promise<Maybe<LoadRun>> => {
      const { run } = await client.getRun({ id: BigInt(id) });
      if (!run) throw new Error(`Run #${id} is not in the history.`);
      return available(storedRun(run));
    },
    scenarios: () =>
      unlessMissing(loadNotYet, async () => {
        const { definitions } = await client.listDefinitions({ kind: DefinitionKind.FILES });
        return definitions.filter((d) => d.path.endsWith('.load.yaml')).map((d) => d.id);
      }),
  };
}
