import { describe, expect, it } from 'vitest';
import type { ListDefinitionsRequest } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { engineEstimate, planLine } from './estimate.ts';

describe('planLine', () => {
  it('counts the tests and files, and the skipped tests when there are any', () => {
    expect(planLine({ tests: 12, files: 3, skipped: 0 })).toBe('12 tests · 3 files');
    expect(planLine({ tests: 1, files: 1, skipped: 2 })).toBe('1 test · 1 file · 2 skipped');
  });

  it('says nothing would run when no test matches', () => {
    expect(planLine({ tests: 0, files: 0, skipped: 4 })).toBe(
      'No test matches. Nothing would run.',
    );
  });
});

describe('engineEstimate', () => {
  it('counts the definitions the selection keeps, their files and the skipped ones', async () => {
    const asked: ListDefinitionsRequest[] = [];
    const estimate = engineEstimate(
      fakeEngine({
        listDefinitions: (request) => {
          asked.push(request);
          return {
            definitions: [
              { id: 'users.crud#a', path: 'services/users/crud.test.yaml' },
              { id: 'users.crud#b', path: 'services/users/crud.test.yaml' },
              { id: 'orders.crud#a', path: 'services/orders/crud.test.yaml' },
              { id: 'orders.crud#b', path: 'services/orders/crud.test.yaml', skipped: true },
            ],
          };
        },
      }),
    );
    const plan = await estimate({ ids: [], paths: [], tags: ['smoke'], changed: 'origin/main' });
    expect(plan).toEqual({ tests: 3, files: 2, skipped: 1 });
    expect(asked[0]?.selection?.tags).toEqual(['smoke']);
    expect(asked[0]?.selection?.changed).toBe('origin/main');
  });
});
