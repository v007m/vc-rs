import type { MessageInitShape } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import type {
  ExportScheduleRequest,
  ScheduleListingSchema,
  WriteScheduleRequest,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { engineSchedulesPort, schedulesNotYet, type ScheduleSpec } from './port.ts';

const nightly: MessageInitShape<typeof ScheduleListingSchema> = {
  schedule: {
    id: 'nightly',
    name: 'Nightly smoke',
    cron: '0 2 * * *',
    selector: ['tag:smoke'],
    profiles: ['prod'],
    notify: [{ kind: 'slack', settings: [{ key: 'channel', value: '#api' }] }],
    runner: 'vcServer',
    enabled: true,
  },
  nextMs: 1_795_732_400_000n,
  last: {
    id: 17n,
    kind: 'run',
    startedMs: 1_795_646_000_000n,
    finishedMs: 1_795_646_004_200n,
    profiles: ['prod'],
    selector: 'tag:smoke',
    status: RunStatus.PASSED,
  },
};

const spec: ScheduleSpec = {
  id: 'weekly',
  cron: '30 6 * * 1',
  selector: [],
  profiles: ['staging'],
  notify: [],
  runner: 'githubActions',
  enabled: true,
};

const change = {
  files: [{ path: 'vibranium.config.yaml', before: 'a\n', after: 'b\n' }],
  titles: ['Add schedule weekly'],
  digest: 'd7',
};

describe('engineSchedulesPort', () => {
  it('lists each schedule with when it next fires and its last run', async () => {
    const port = engineSchedulesPort(
      fakeEngine({ listSchedules: () => ({ schedules: [nightly, {}] }) }),
    );
    const got = await port.list();
    expect(got).toEqual({
      available: true,
      value: [
        expect.objectContaining({
          id: 'nightly',
          name: 'Nightly smoke',
          selector: ['tag:smoke'],
          notify: [{ kind: 'slack', settings: [{ key: 'channel', value: '#api' }] }],
          nextMs: 1_795_732_400_000,
          last: expect.objectContaining({ id: 17, outcome: 'passed', durationMs: 4200 }),
        }),
      ],
    });
  });

  it('previews a change as a dry run, then writes it with the preview digest', async () => {
    const asked: WriteScheduleRequest[] = [];
    const port = engineSchedulesPort(
      fakeEngine({
        writeSchedule: (request) => {
          asked.push(request);
          return change;
        },
      }),
    );
    const dry = await port.preview({ put: spec });
    expect(dry).toMatchObject({ available: true, value: { digest: 'd7' } });
    await port.write({ put: spec }, 'd7');
    await port.write({ remove: 'nightly' }, 'd8');
    expect(
      asked.map(({ schedule, remove, write, expectedDigest }) => [
        schedule?.id,
        remove,
        write,
        expectedDigest,
      ]),
    ).toEqual([
      ['weekly', undefined, false, undefined],
      ['weekly', undefined, true, 'd7'],
      [undefined, 'nightly', true, 'd8'],
    ]);
    expect(asked[0]?.schedule).toMatchObject({ runner: 'githubActions', profiles: ['staging'] });
  });

  it('previews the workflow, then writes it with the digest and gives its path', async () => {
    const asked: ExportScheduleRequest[] = [];
    const port = engineSchedulesPort(
      fakeEngine({
        exportSchedule: (request) => {
          asked.push(request);
          return { ...change, path: '.github/workflows/vibranium-nightly.yml' };
        },
      }),
    );
    expect(await port.previewExport('nightly')).toMatchObject({ value: { digest: 'd7' } });
    expect(await port.export('nightly', 'd7')).toEqual({
      available: true,
      value: '.github/workflows/vibranium-nightly.yml',
    });
    expect(asked.map(({ id, write, expectedDigest }) => [id, write, expectedDigest])).toEqual([
      ['nightly', false, undefined],
      ['nightly', true, 'd7'],
    ]);
  });

  it('says the engine is too old when it lacks the schedule RPCs', async () => {
    const port = engineSchedulesPort(fakeEngine({}));
    const missing = { available: false, reason: schedulesNotYet };
    expect(await port.list()).toEqual(missing);
    expect(await port.preview({ remove: 'nightly' })).toEqual(missing);
    expect(await port.export('nightly', 'd1')).toEqual(missing);
  });
});
