import { describe, expect, it } from 'vitest';
import { RunHistory, selectorsOf } from './history.svelte.ts';
import type { HistoryPort, RunLine } from './port.ts';

function line(id: number, selector: string): RunLine {
  const counts = { passed: 1, failed: 0, blocked: 0, flaky: 0, skipped: 0 };
  return { id, kind: 'run', startedMs: id, profiles: [], selector, outcome: 'passed', counts };
}

describe('selectorsOf', () => {
  it('counts each selector once, in the order the newest runs used them', () => {
    const runs = [line(4, 'users.crud'), line(3, ''), line(2, 'users.crud'), line(1, 'tag:smoke')];
    expect(selectorsOf(runs)).toEqual([
      { selector: 'users.crud', runs: 2 },
      { selector: '', runs: 1 },
      { selector: 'tag:smoke', runs: 1 },
    ]);
    expect(selectorsOf([])).toEqual([]);
  });
});

describe('RunHistory.filter', () => {
  it('drops a filter the runs read again no longer use', async () => {
    let runs = [line(2, 'tag:smoke'), line(1, '')];
    const port: HistoryPort = {
      runs: () => Promise.resolve(runs),
      run: () => Promise.reject(new Error('unused')),
    };
    const history = new RunHistory(port);
    await history.load();
    history.filter('tag:smoke');
    expect(history.shown.map((run) => run.id)).toEqual([2]);
    await history.load();
    expect(history.selector).toBe('tag:smoke');
    runs = [line(3, ''), line(1, '')];
    await history.load();
    expect(history.selector).toBeUndefined();
    expect(history.shown.map((run) => run.id)).toEqual([3, 1]);
  });
});
