import { describe, expect, it } from 'vitest';
import { emptyForm, runCommand, runRequest, selectorParts } from './form.ts';

describe('run form', () => {
  it('sorts the selector into IDs, paths and tags', () => {
    expect(selectorParts('users.crud#gets, services/orders/ tag:smoke orders.*')).toEqual({
      ids: ['users.crud#gets', 'orders.*'],
      paths: ['services/orders/'],
      tags: ['smoke'],
    });
  });

  it('asks for only what the form sets, so the engine keeps its defaults', () => {
    expect(runRequest(emptyForm)).toEqual({
      selection: { ids: [], paths: [], tags: [] },
      profiles: [],
      bail: false,
      strict: false,
      reports: [],
    });
    const full = {
      ...emptyForm,
      selector: 'tag:smoke',
      profiles: ['staging', 'eu'],
      concurrency: 4,
      bail: true,
      changed: 'origin/main',
      seed: 42,
      repeat: 3,
      strict: true,
      reports: ['junit', 'html'] as const,
    };
    expect(runRequest(full)).toEqual({
      selection: { ids: [], paths: [], tags: ['smoke'], changed: 'origin/main' },
      profiles: ['staging', 'eu'],
      bail: true,
      strict: true,
      reports: ['junit', 'html'],
      concurrency: 4,
      repeat: 3,
      seed: 42n,
    });
    expect(runCommand(full)).toBe(
      'vc run -t smoke --profile staging,eu --concurrency 4 --bail --changed origin/main --seed 42 --repeat 3 --strict --report junit,html',
    );
  });

  it('quotes a glob so the shell does not expand it', () => {
    expect(runCommand({ ...emptyForm, selector: 'users.* orders.crud' })).toBe(
      "vc run 'users.*' orders.crud",
    );
  });
});
