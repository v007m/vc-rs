import { create } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import { DefinitionSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import {
  changedSince,
  facets,
  groupByFile,
  matches,
  parseQuery,
  toggleFilter,
  toRow,
} from './catalog.ts';

const rows = [
  {
    id: 'users.crud#createsUser',
    name: 'Creates a user',
    path: 'services/users/crud.test.yaml',
    tags: ['smoke'],
  },
  { id: 'users.crud#rejectsBadEmail', path: 'services/users/crud.test.yaml', tags: ['slow path'] },
  {
    id: 'orders.place#placesOrder',
    path: 'services/orders/place.test.yaml',
    tags: ['smoke'],
    skipped: true,
  },
].map((init) => toRow(create(DefinitionSchema, init)));

function ids(text: string): string[] {
  const query = parseQuery(text);
  return rows.filter((row) => matches(row, query)).map((row) => row.local);
}

describe('catalog', () => {
  it('splits the full id and derives the service', () => {
    expect(rows[1]).toMatchObject({
      fileId: 'users.crud',
      local: 'rejectsBadEmail',
      name: 'rejectsBadEmail',
      service: 'users',
    });
  });

  it('matches bare words against id, name and file, ignoring case', () => {
    expect(ids('CREATES')).toEqual(['createsUser']);
    expect(ids('orders')).toEqual(['placesOrder']);
  });

  it('combines filters with AND and reads quoted values', () => {
    expect(ids('tag:smoke service:users')).toEqual(['createsUser']);
    expect(ids('tag:"slow path"')).toEqual(['rejectsBadEmail']);
    expect(ids('is:skipped')).toEqual(['placesOrder']);
    expect(ids('file:users')).toEqual(['createsUser', 'rejectsBadEmail']);
  });

  it('keeps the tests the engine found changed since a ref, and none until it answers', () => {
    const query = parseQuery('changed:main tag:smoke');
    const changed = new Set(['users.crud#createsUser', 'orders.place#placesOrder']);
    const context = { changed: (since: string) => (since === 'main' ? changed : undefined) };
    expect(rows.filter((row) => matches(row, query, context)).map((r) => r.local)).toEqual([
      'createsUser',
      'placesOrder',
    ]);
    expect(ids('changed:main')).toEqual([]);
    expect(changedSince(parseQuery('changed:main changed:HEAD~3 changed:main'))).toEqual([
      'main',
      'HEAD~3',
    ]);
  });

  it('reads an unknown key as a bare word', () => {
    expect(parseQuery('owner:me')).toEqual({ words: ['owner:me'], filters: [] });
  });

  it('groups by file in first-seen order', () => {
    expect(groupByFile(rows).map((g) => [g.fileId, g.tests.length])).toEqual([
      ['users.crud', 2],
      ['orders.place', 1],
    ]);
  });

  it('counts facets, most common first', () => {
    expect(facets(rows, (r) => r.tags)).toEqual([
      { value: 'smoke', count: 2 },
      { value: 'slow path', count: 1 },
    ]);
  });

  it('toggles a filter term in the query text', () => {
    expect(toggleFilter('users', 'tag', 'smoke')).toBe('users tag:smoke');
    expect(toggleFilter('users tag:smoke', 'tag', 'smoke')).toBe('users');
    expect(toggleFilter('', 'tag', 'slow path')).toBe('tag:"slow path"');
  });
});
