import type { MessageInitShape } from '@bufbuild/protobuf';
import type { Client } from '@connectrpc/connect';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import type {
  EngineService,
  ScheduleListing,
  ScheduleSchema,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { runLine, type RunLine } from '#lib/history/port.ts';
import { preview, type Preview } from '#lib/writes/staged.svelte.ts';

/** Why the Schedules screen is empty on an engine without the schedule RPCs. */
export const schedulesNotYet = 'Schedules need a newer engine.';

/** Where a run that fails is sent: a channel kind and its settings as written (secrets as references). */
export type NotifyLine = Readonly<{
  kind: string;
  settings: readonly Readonly<{ key: string; value: string }>[];
}>;

/** The runners a schedule can name (D700). */
export const runners = ['vcServer', 'githubActions'] as const;

/** A runner as the screen names it. */
export const runnerLabel: Readonly<Record<string, string>> = {
  vcServer: 'vc ui server',
  githubActions: 'GitHub Actions',
};

/** One schedule as the config writes it (D705). */
export type ScheduleSpec = Readonly<{
  id: string;
  name?: string;
  cron: string;
  selector: readonly string[];
  profiles: readonly string[];
  notify: readonly NotifyLine[];
  runner: string;
  enabled: boolean;
}>;

/** A schedule as the list shows it: the spec, when it next fires and its newest run. */
export type ScheduleLine = ScheduleSpec & Readonly<{ nextMs?: number; last?: RunLine }>;

/** A config write: one schedule added or put in place of the one with its `id`, or one removed. */
export type ScheduleChange = Readonly<{ put: ScheduleSpec }> | Readonly<{ remove: string }>;

/** What the Schedules screen asks of the engine; each answer says when the engine lacks the RPC. */
export interface SchedulesPort {
  list(): Promise<Maybe<ScheduleLine[]>>;
  /** The dry run of `change`: the config diff and its digest. */
  preview(change: ScheduleChange): Promise<Maybe<Preview>>;
  /** Writes `change` unless the config changed since the preview that gave `digest`. */
  write(change: ScheduleChange, digest: string): Promise<Maybe<Preview>>;
  /** The dry run of schedule `id`'s GitHub Actions workflow. */
  previewExport(id: string): Promise<Maybe<Preview>>;
  /** Writes the workflow; gives its path, relative to the project root. */
  export(id: string, digest: string): Promise<Maybe<string>>;
}

/** A wire listing as the list shows it; a listing without a schedule is skipped. */
export function scheduleLines({ schedule, nextMs, last }: ScheduleListing): ScheduleLine[] {
  if (!schedule) return [];
  const { id, name, cron, selector, profiles, notify, runner, enabled } = schedule;
  return [
    {
      id,
      ...(name ? { name } : {}),
      cron,
      selector,
      profiles,
      notify: notify.map(({ kind, settings }) => ({
        kind,
        settings: settings.map(({ key, value }) => ({ key, value })),
      })),
      runner,
      enabled,
      ...(nextMs === undefined ? {} : { nextMs: Number(nextMs) }),
      ...(last ? { last: runLine(last) } : {}),
    },
  ];
}

/** A spec as the wire writes it, without what only the list knows. */
export function wireSchedule(spec: ScheduleSpec): MessageInitShape<typeof ScheduleSchema> {
  const { id, name, cron, selector, profiles, notify, runner, enabled } = spec;
  return {
    id,
    ...(name ? { name } : {}),
    cron,
    selector: [...selector],
    profiles: [...profiles],
    notify: notify.map(({ kind, settings }) => ({ kind, settings: [...settings] })),
    runner,
    enabled,
  };
}

const target = (change: ScheduleChange) =>
  'put' in change ? { schedule: wireSchedule(change.put) } : { remove: change.remove };

export type SchedulesClient = Pick<
  Client<typeof EngineService>,
  'listSchedules' | 'writeSchedule' | 'exportSchedule'
>;

/** The port over the engine client's ListSchedules, WriteSchedule and ExportSchedule (D705–D707). */
export function engineSchedulesPort(client: SchedulesClient): SchedulesPort {
  const missing = <T>(work: () => Promise<T>) => unlessMissing(schedulesNotYet, work);
  return {
    list: () =>
      missing(async () => (await client.listSchedules({})).schedules.flatMap(scheduleLines)),
    preview: (change) =>
      missing(async () => preview(await client.writeSchedule({ ...target(change), write: false }))),
    write: (change, digest) =>
      missing(async () =>
        preview(
          await client.writeSchedule({ ...target(change), write: true, expectedDigest: digest }),
        ),
      ),
    previewExport: (id) =>
      missing(async () => preview(await client.exportSchedule({ id, write: false }))),
    export: (id, digest) =>
      missing(
        async () => (await client.exportSchedule({ id, write: true, expectedDigest: digest })).path,
      ),
  };
}
