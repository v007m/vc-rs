import { describe, expect, it } from 'vitest';
import { emptyForm, formOf, idProblem, specFrom } from './form.ts';

describe('idProblem', () => {
  it('takes a new camelCase ID', () => {
    expect(idProblem('nightlySmoke2', ['weekly'])).toBeUndefined();
  });

  it('says why an ID cannot name a new schedule', () => {
    expect(idProblem('', [])).toBe('Give the schedule an ID.');
    expect(idProblem('Nightly', [])).toMatch(/camelCase/);
    expect(idProblem('nightly-smoke', [])).toMatch(/camelCase/);
    expect(idProblem('nightly', ['nightly'])).toBe('A schedule named nightly exists already.');
  });
});

describe('specFrom and formOf', () => {
  const notify = [{ kind: 'slack', settings: [{ key: 'channel', value: '#api' }] }];

  it('splits the selector, tidies the cron and drops an empty name', () => {
    const spec = specFrom({
      ...emptyForm,
      id: ' nightly ',
      name: '  ',
      cron: ' 0  2 * * * ',
      selector: ' tag:smoke   users.crud ',
      profiles: ['prod'],
    });
    expect(spec).toEqual({
      id: 'nightly',
      cron: '0 2 * * *',
      selector: ['tag:smoke', 'users.crud'],
      profiles: ['prod'],
      notify: [],
      runner: 'vcServer',
      enabled: true,
    });
  });

  it('round-trips a schedule, keeping the notify channels the form does not show', () => {
    const spec = {
      id: 'weekly',
      name: 'Weekly',
      cron: '30 6 * * 1',
      selector: ['tag:full'],
      profiles: ['staging', 'prod'],
      notify,
      runner: 'githubActions',
      enabled: false,
    };
    const form = formOf(spec);
    expect(form.selector).toBe('tag:full');
    expect(specFrom(form, spec.notify)).toEqual(spec);
  });
});
