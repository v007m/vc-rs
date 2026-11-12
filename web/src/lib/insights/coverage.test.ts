import { describe, expect, it } from 'vitest';
import { codes, coverageRows, covers, testedShare, testsLabel } from './coverage.ts';
import type { ApiTrend, CoveredApi } from './port.ts';

const api = (apiId: string, documented: string[], more: Partial<CoveredApi> = {}): CoveredApi => ({
  apiId,
  method: 'GET',
  path: '/users',
  file: 'services/users/users.api.yaml',
  tests: 1,
  skipped: 0,
  documented,
  specOnly: false,
  ...more,
});

const trend = (apiId: string, calls: number, statuses: number[]): ApiTrend => ({
  apiId,
  method: 'GET',
  calls,
  statuses,
});

describe('coverage', () => {
  it('covers a code with itself and with its class, and nothing else', () => {
    expect([covers('200', 200), covers('2xx', 201), covers('2xx', 200)]).toEqual([
      true,
      true,
      true,
    ]);
    expect([covers('200', 201), covers('4xx', 500), covers('2xx', 300)]).toEqual([
      false,
      false,
      false,
    ]);
  });

  it('marks each documented status seen or missed, then adds the seen ones none covers', () => {
    expect(codes(['2xx', '404', '409'], [201, 404, 503])).toEqual([
      { text: '2xx', kind: 'seen' },
      { text: '404', kind: 'seen' },
      { text: '409', kind: 'missed' },
      { text: '503', kind: 'undocumented' },
    ]);
  });

  it('puts the apis with no call in the window first, a spec-only operation always among them', () => {
    const rows = coverageRows(
      [
        api('users#list', ['200']),
        api('users#get', ['200', '404']),
        api('users#remove', []),
        api('createUser', ['201'], { specOnly: true, tests: 0 }),
      ],
      [trend('users#list', 4, [200]), trend('users#get', 2, [200]), trend('users#remove', 0, [])],
    );
    expect(rows.map((r) => [r.apiId, r.untested, r.gap])).toEqual([
      ['users#remove', true, true],
      ['createUser', true, true],
      ['users#list', false, false],
      ['users#get', false, true],
    ]);
    expect(rows[3]?.codes).toEqual([
      { text: '200', kind: 'seen' },
      { text: '404', kind: 'missed' },
    ]);
  });

  it('counts a call that got no response as a call, so the api is not untested', () => {
    const [row] = coverageRows([api('users#list', ['200'])], [trend('users#list', 1, [])]);
    expect(row?.untested).toBe(false);
    expect(row?.codes).toEqual([{ text: '200', kind: 'missed' }]);
  });

  it('labels the tests with the skipped ones, and a spec-only operation as having no api', () => {
    expect(testsLabel(api('a#one', [], { tests: 3, skipped: 1 }))).toBe('3 (1 skipped)');
    expect(testsLabel(api('a#one', [], { tests: 0 }))).toBe('0');
    expect(testsLabel(api('one', [], { specOnly: true }))).toBe('no api');
  });

  it('gives the share of rows called in the window, and none for no rows', () => {
    const rows = coverageRows(
      [api('a#one', []), api('a#two', []), api('a#three', [])],
      [trend('a#one', 1, [200])],
    );
    expect([testedShare(rows), testedShare([])]).toEqual([33, undefined]);
  });
});
