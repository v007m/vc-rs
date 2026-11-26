import { describe, expect, it } from 'vitest';
import { childPath, filterBody, jsonRows, jsonType, type FilterHits } from './json.ts';

const body = { orders: [{ id: 1, total: 9.5 }, { id: 2 }], 'odd key': null };

describe('JSON tree', () => {
  it('names a child by key, index, or a quoted key when it is not an identifier', () => {
    expect([childPath('$', 'id'), childPath('$.a', 0), childPath('$', 'odd key')]).toEqual([
      '$.id',
      '$.a[0]',
      '$["odd key"]',
    ]);
  });

  it('flattens a body into opening, value and closing lines with their paths and depth', () => {
    const rows = jsonRows(body);
    expect(rows.map((r) => [r.kind, r.path, r.depth])).toEqual([
      ['open', '$', 0],
      ['open', '$.orders', 1],
      ['open', '$.orders[0]', 2],
      ['leaf', '$.orders[0].id', 3],
      ['leaf', '$.orders[0].total', 3],
      ['close', '$.orders[0]', 2],
      ['open', '$.orders[1]', 2],
      ['leaf', '$.orders[1].id', 3],
      ['close', '$.orders[1]', 2],
      ['close', '$.orders', 1],
      ['leaf', '$["odd key"]', 1],
      ['close', '$', 0],
    ]);
    expect(rows[1]).toMatchObject({ key: 'orders', bracket: '[', size: 2, collapsed: false });
    expect(jsonRows('plain')).toEqual([{ path: '$', depth: 0, kind: 'leaf', value: 'plain' }]);
  });

  it('skips what sits under a collapsed path and keeps its size', () => {
    const rows = jsonRows(body, new Set(['$.orders']));
    expect(rows.map((r) => r.path)).toEqual(['$', '$.orders', '$["odd key"]', '$']);
    expect(rows[1]).toMatchObject({ collapsed: true, size: 2 });
  });

  it('matches a JSONPath filter with wildcards, and keeps the lines above and under a hit', () => {
    const rows = jsonRows(body);
    const ids = filterBody(rows, '$.orders[*].id') as FilterHits;
    expect([...ids.exact]).toEqual(['$.orders[0].id', '$.orders[1].id']);
    expect(ids.hits.has('$.orders')).toBe(true);
    expect(ids.hits.has('$.orders[0].total')).toBe(false);
    const bare = filterBody(rows, 'orders[0].*') as FilterHits;
    expect([...bare.exact]).toEqual(['$.orders[0].id', '$.orders[0].total']);
    expect([...(filterBody(rows, 'body.orders') as FilterHits).exact]).toEqual(['$.orders']);
  });

  it('leaves an empty filter off and hands an expression to CEL', () => {
    const rows = jsonRows(body);
    expect(filterBody(rows, '  ')).toBeUndefined();
    expect(filterBody(rows, 'body.orders.size() > 1')).toEqual({ cel: true });
  });

  it('names a value by the JSON type a check uses', () => {
    expect([null, [], true, 1, 's', {}].map(jsonType)).toEqual([
      'null',
      'list',
      'bool',
      'number',
      'string',
      'object',
    ]);
  });
});
