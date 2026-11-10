import { describe, expect, it } from 'vitest';
import { TestStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { events } from '#lib/testing/engine.ts';
import { apply, eventCap, startingRun, type RunView } from './model.ts';

function play(view: RunView, ...kinds: Parameters<typeof events>): RunView {
  return events(...kinds).reduce(apply, view);
}

const finished = (id: string, instance: number | undefined) =>
  ({
    case: 'testFinished',
    value: { test: { id, label: 'same', status: TestStatus.PASSED, durationMs: 7 }, instance },
  }) as const;

describe('run model', () => {
  it('finds each finished test by its plan instance, whatever its label', () => {
    const view = play(
      startingRun(),
      {
        case: 'runPlanned',
        value: {
          instances: [
            { instance: 1, test: 'users.crud#rejects', label: '', exampleRow: 0 },
            { instance: 2, test: 'users.crud#rejects', label: '', exampleRow: 1 },
            { instance: 3, test: 'users.crud#lists', label: 'same' },
            { instance: 4, test: 'users.crud#lists', label: 'same', repeatIndex: 1 },
            { instance: 5, test: 'orders.crud#lists', label: 'same' },
          ],
        },
      },
      { case: 'actionStarted', value: { owner: { kind: { case: 'instance', value: 2 } } } },
      finished('users.crud#rejects', 2),
      finished('users.crud#lists', 4),
      finished('orders.crud#lists', 5),
      finished('orders.crud#skipped', undefined),
    );
    expect(view.plan.map((row) => [row.instance, row.label, row.state, row.ms])).toEqual([
      [1, 'users.crud#rejects', 'queued', undefined],
      [2, 'users.crud#rejects', 'passed', 7],
      [3, 'same', 'queued', undefined],
      [4, 'same', 'passed', 7],
      [5, 'same', 'passed', 7],
    ]);
    expect(view.counts.passed).toBe(4);
  });

  it('keeps the last lines of a long stream and counts the rest', () => {
    const files = Array.from({ length: eventCap * 5 }, (_, i) => ({
      case: 'fileStarted' as const,
      value: { file: `f${i}` },
    }));
    const view = play(startingRun(), ...files);
    expect(view.events).toHaveLength(eventCap);
    expect(view.dropped).toBe(eventCap * 4);
    expect(view.events.at(-1)?.who).toBe(`f${eventCap * 5 - 1}`);
    expect(view.events[0]?.who).toBe(`f${eventCap * 4}`);
  });
});
