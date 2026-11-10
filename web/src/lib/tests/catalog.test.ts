import { create } from '@bufbuild/protobuf';
import { ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { DefinitionSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Catalog } from './catalog.svelte.ts';
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

describe('the catalog store', () => {
  function store(gate?: Promise<void>) {
    const asked: string[] = [];
    const catalog = new Catalog(
      fakeEngine({
        listDefinitions: async (request) => {
          const since = request.selection?.changed;
          if (!since) return { definitions: [] };
          asked.push(since);
          if (since === 'bad') throw new ConnectError('unknown revision bad');
          await gate;
          return { definitions: [{ id: 'users.crud#createsUser' }] };
        },
      }),
    );
    return { catalog, asked };
  }

  it('asks once per ref, and again after it loads, since a save may have changed the files', async () => {
    const { catalog, asked } = store();
    await catalog.loadChanged('main');
    await catalog.loadChanged('main');
    expect([...(catalog.changed.get('main') ?? [])]).toEqual(['users.crud#createsUser']);
    await catalog.load();
    expect(catalog.changed.size).toBe(0);
    await catalog.loadChanged('main');
    expect(asked).toEqual(['main', 'main']);
  });

  it('keeps why each ref failed apart, and asks a failed ref again', async () => {
    const { catalog, asked } = store();
    await catalog.loadChanged('bad');
    await catalog.loadChanged('main');
    expect([...catalog.changedErrors]).toEqual([['bad', 'unknown revision bad']]);
    await catalog.loadChanged('bad');
    expect(asked).toEqual(['bad', 'main', 'bad']);
    await catalog.load();
    expect(catalog.changedErrors.size).toBe(0);
  });

  it('drops an answer that arrives after it loads again', async () => {
    let open = () => {};
    const { catalog } = store(new Promise((resolve) => (open = resolve)));
    const pending = catalog.loadChanged('main');
    await catalog.load();
    open();
    await pending;
    expect(catalog.changed.has('main')).toBe(false);
  });
});
