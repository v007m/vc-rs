import { describe, expect, it } from 'vitest';
import { ALL, distinct, failedIds, matches, RECENT, RunHistory } from './history.svelte.ts';
import type { HistoryPort, RunLine, RunScope, TestDetail } from './port.ts';

function line(id: number, selector: string, extra: Partial<RunLine> = {}): RunLine {
  const counts = { passed: 1, failed: 0, blocked: 0, flaky: 0, skipped: 0 };
  return {
    id,
    kind: 'run',
    startedMs: id,
    profiles: [],
    selector,
    outcome: 'passed',
    counts,
    ...extra,
  };
}

describe('matches', () => {
  const run = line(7, 'tag:smoke', { profiles: ['staging'], branch: 'main', outcome: 'failed' });

  it('keeps a run with every word, in any of its number, selector, profile or branch', () => {
    expect(matches(run, { query: 'Smoke staging' })).toBe(true);
    expect(matches(run, { query: '#7 main' })).toBe(true);
    expect(matches(run, { query: 'smoke prod' })).toBe(false);
  });

  it('keeps only the outcome asked for', () => {
    expect(matches(run, { outcome: 'failed' })).toBe(true);
    expect(matches(run, { outcome: 'passed' })).toBe(false);
  });
});

describe('RunHistory', () => {
  function setup(runs: RunLine[]) {
    const asked: [number, RunScope | undefined][] = [];
    const port: HistoryPort = {
      runs: (limit, scope) => {
        asked.push([limit, scope]);
        return Promise.resolve(runs);
      },
      run: () => Promise.reject(new Error('run #9 is gone')),
    };
    return { history: new RunHistory(port), asked };
  }

  it('reads the recent runs whole and All runs in its scope', async () => {
    const { history, asked } = setup([line(2, 'a'), line(1, 'b')]);
    await history.rescope({ kind: 'load' });
    await history.load();
    expect(asked).toEqual([
      [ALL, { kind: 'load' }],
      [RECENT, undefined],
      [ALL, { kind: 'load' }],
    ]);
    history.filter = { query: 'b' };
    expect(history.shown.map((run) => run.id)).toEqual([1]);
  });

  it('finds the run before one, of the same kind, to compare with', async () => {
    const { history } = setup([line(5, ''), line(4, '', { kind: 'load' }), line(3, '')]);
    await history.load();
    expect(history.previous(5)?.id).toBe(3);
    expect(history.previous(3)).toBeUndefined();
  });

  it('keeps All runs for the newest scope when an older listing answers last', async () => {
    const answers: ((runs: RunLine[]) => void)[] = [];
    const port: HistoryPort = {
      runs: (limit) =>
        limit === RECENT ? Promise.resolve([]) : new Promise((done) => answers.push(done)),
      run: () => Promise.reject(new Error('unused')),
    };
    const history = new RunHistory(port);
    const unscoped = history.load();
    const scoped = history.rescope({ schedule: 'nightly' });
    answers[1]?.([line(3, 'tag:smoke')]);
    await scoped;
    answers[0]?.([line(9, 'all'), line(8, 'all')]);
    await unscoped;
    expect(history.runs.map((run) => run.id)).toEqual([3]);
  });

  it('says why a run could not be read', async () => {
    const { history } = setup([]);
    await history.show(9);
    expect(history.error).toBe('run #9 is gone');
  });
});

describe('distinct and failedIds', () => {
  it('lists each value once', () => {
    const runs = [line(2, '', { profiles: ['dev'] }), line(1, '', { profiles: ['dev', 'prod'] })];
    expect(distinct(runs, (run) => run.profiles)).toEqual(['dev', 'prod']);
  });

  it('re-runs each failed or blocked test once', () => {
    const test = (id: string, status: TestDetail['row']['status']) =>
      ({ id, row: { status } }) as TestDetail;
    expect(failedIds([test('a', 'failed'), test('b', 'passed'), test('a', 'blocked')])).toEqual([
      'a',
    ]);
  });
});
