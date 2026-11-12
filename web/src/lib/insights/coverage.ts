import type { ApiTrend, CoveredApi } from './port.ts';

/**
 * One status of a coverage row: documented and seen in the window (`seen`), documented and never
 * seen (`missed`), or seen and covered by no documented status (`undocumented`).
 */
export type Code = Readonly<{ text: string; kind: 'seen' | 'missed' | 'undocumented' }>;

/** A coverage row as Insights shows it, against the exchanges of the window. */
export type CoverageRow = CoveredApi &
  Readonly<{
    /** No exchange of it is stored in the window (D301). */
    untested: boolean;
    codes: readonly Code[];
    /** Untested, or a documented status was never seen. */
    gap: boolean;
  }>;

/** Whether the documented `pattern`, `200` or `2xx`, covers the status `seen`: a class covers its codes (D300). */
export function covers(pattern: string, seen: number): boolean {
  const klass = /^([1-5])xx$/.exec(pattern);
  return klass ? Math.floor(seen / 100) === Number(klass[1]) : Number(pattern) === seen;
}

/** The documented statuses, each seen or missed, then each seen one no documented status covers. */
export function codes(documented: readonly string[], seen: readonly number[]): Code[] {
  const marked = documented.map((text): Code => ({
    text,
    kind: seen.some((s) => covers(text, s)) ? 'seen' : 'missed',
  }));
  const undocumented = seen
    .filter((s) => !documented.some((d) => covers(d, s)))
    .map((s): Code => ({ text: String(s), kind: 'undocumented' }));
  return [...marked, ...undocumented];
}

/** Each covered api against the window's trends, the untested ones first, otherwise in order. */
export function coverageRows(
  apis: readonly CoveredApi[],
  trends: readonly ApiTrend[],
): CoverageRow[] {
  const byId = new Map(trends.map((t) => [t.apiId, t]));
  const rows = apis.map((api): CoverageRow => {
    const trend = api.specOnly ? undefined : byId.get(api.apiId);
    const untested = (trend?.calls ?? 0) === 0;
    const marked = codes(api.documented, trend?.statuses ?? []);
    return {
      ...api,
      untested,
      codes: marked,
      gap: untested || marked.some((c) => c.kind === 'missed'),
    };
  });
  return rows.sort((a, b) => Number(b.untested) - Number(a.untested));
}

/** The Tests column: `3`, `3 (1 skipped)`; an operation only a snapshot has has no api to test. */
export function testsLabel(row: CoveredApi): string {
  if (row.specOnly) return 'no api';
  return row.skipped > 0 ? `${row.tests} (${row.skipped} skipped)` : String(row.tests);
}

/** The share of rows with an exchange in the window, in whole percent; `undefined` when there are none. */
export function testedShare(rows: readonly CoverageRow[]): number | undefined {
  if (rows.length === 0) return undefined;
  return Math.round((rows.filter((r) => !r.untested).length * 100) / rows.length);
}
