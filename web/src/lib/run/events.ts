import type { Owner, RunEvent } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { TestStatus, type Failure } from '#lib/gen/vibranium/results/v1/results_pb.ts';

/** How much an event line matters; the log filters on it. */
export type Level = 'info' | 'warn' | 'error';

/** One line of the run's event log. */
export type EventLine = Readonly<{
  seq: bigint;
  level: Level;
  /** What the event is about: a test, a file, an action, or the run. */
  who: string;
  message: string;
}>;

type Describe = (event: RunEvent) => Omit<EventLine, 'seq'> | undefined;

const run = 'run';

function ownerLabel(owner: Owner | undefined): string {
  return owner?.label || run;
}

function failureText(failure: Failure): string {
  return failure.code ? `${failure.code} ${failure.message}` : failure.message;
}

const testWords: Record<TestStatus, [Level, string]> = {
  [TestStatus.UNSPECIFIED]: ['info', 'finished'],
  [TestStatus.PASSED]: ['info', 'passed'],
  [TestStatus.FAILED]: ['error', 'failed'],
  [TestStatus.BLOCKED]: ['warn', 'blocked'],
  [TestStatus.FLAKY]: ['warn', 'passed on a retry (flaky)'],
  [TestStatus.SKIPPED]: ['info', 'skipped'],
  [TestStatus.ALLOWED_FAILURE]: ['warn', 'allowed to fail'],
};

/** How each event kind reads in the log; kinds not listed are not shown. */
const lines: Partial<Record<NonNullable<RunEvent['kind']['case']>, Describe>> = {
  runStarted: ({ kind }) =>
    kind.case === 'runStarted'
      ? {
          level: 'info',
          who: run,
          message: `started: ${kind.value.instances} tests in ${kind.value.files} files`,
        }
      : undefined,
  fileStarted: ({ kind }) =>
    kind.case === 'fileStarted'
      ? { level: 'info', who: kind.value.file, message: 'started' }
      : undefined,
  actionFinished: ({ kind }) =>
    kind.case === 'actionFinished' && kind.value.failure
      ? {
          level: 'error',
          who: ownerLabel(kind.value.owner),
          message: `${kind.value.action?.label ?? 'action'} failed: ${failureText(kind.value.failure)}`,
        }
      : undefined,
  warned: ({ kind }) =>
    kind.case === 'warned' && kind.value.warning
      ? {
          level: 'warn',
          who: ownerLabel(kind.value.owner),
          message: failureText(kind.value.warning),
        }
      : undefined,
  cleanupFinished: ({ kind }) =>
    kind.case === 'cleanupFinished' && kind.value.failure
      ? {
          level: 'warn',
          who: ownerLabel(kind.value.owner),
          message: `cleanup failed: ${failureText(kind.value.failure)}`,
        }
      : undefined,
  testFinished: ({ kind }) => {
    const test = kind.case === 'testFinished' ? kind.value.test : undefined;
    if (!test) return undefined;
    const [level, word] = testWords[test.status];
    const why = test.failure ? `: ${failureText(test.failure)}` : '';
    return { level, who: test.label || test.id, message: `${word}${why}` };
  },
  missed: ({ kind }) =>
    kind.case === 'missed'
      ? { level: 'warn', who: run, message: `${kind.value.count} events were dropped` }
      : undefined,
};

/** The log line for one event, or `undefined` for events the log does not show. */
export function describe(event: RunEvent): EventLine | undefined {
  const line = event.kind.case && lines[event.kind.case]?.(event);
  return line ? { seq: event.seq, ...line } : undefined;
}

/** The lowest level each log filter shows. */
export const levelFilters = [
  { label: 'All', min: 'info' },
  { label: 'Warnings', min: 'warn' },
  { label: 'Errors', min: 'error' },
] as const satisfies readonly { label: string; min: Level }[];

const rank: Record<Level, number> = { info: 0, warn: 1, error: 2 };

/** The lines at `min` or above whose text contains `text`, newest first. */
export function visibleLines(lines: readonly EventLine[], min: Level, text: string): EventLine[] {
  const needle = text.trim().toLowerCase();
  return lines
    .filter((line) => rank[line.level] >= rank[min])
    .filter((line) => !needle || `${line.who} ${line.message}`.toLowerCase().includes(needle))
    .reverse();
}
