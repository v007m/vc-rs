import { describe, expect, it } from 'vitest';
import { MASKED } from './port.ts';
import { userKeyProblem, userRows, withEdit } from './userKeys.ts';
import { scopeOf } from './userScope.ts';

describe('userKeyProblem', () => {
  it('takes profile, defaults.*, reports.* and vars.<name>', () => {
    for (const key of [
      'profile',
      'defaults.timeout',
      'defaults.retry.attempts',
      'reports.dir',
      'reports.html.layout',
      'vars.region',
      'vars._owner2',
    ]) {
      expect(userKeyProblem(key), key).toBeUndefined();
    }
  });

  it('says which keys it takes for any other', () => {
    for (const key of ['plugins.allowUnconfined', 'defaults', 'defaults.', 'profiles.dev', 'x']) {
      expect(userKeyProblem(key), key).toBe(
        'Use profile, defaults.<name>, reports.<name> or vars.<name>.',
      );
    }
    expect(userKeyProblem('')).toBe('Enter a key.');
  });

  it('explains a var name that does not fit', () => {
    for (const key of ['vars.2fa', 'vars.a-b', 'vars.', 'vars.a.b']) {
      expect(userKeyProblem(key), key).toMatch(/^A var name starts with a letter or _/);
    }
  });
});

describe('userRows', () => {
  const settings = [
    { key: 'defaults.timeout', value: '12s', shadowed: true },
    { key: 'vars.token', value: MASKED, shadowed: false },
    { key: 'profile', value: 'dev', shadowed: false },
  ];

  it('marks a masked value and stages edits over the file', () => {
    const edits = [
      { key: 'profile', value: 'qa' },
      { key: 'vars.token' },
      { key: 'vars.region', value: 'eu' },
    ];
    expect(userRows(settings, edits)).toEqual([
      { key: 'defaults.timeout', value: '12s', shadowed: true, masked: false },
      { key: 'vars.token', value: MASKED, shadowed: false, masked: true, change: 'removed' },
      { key: 'profile', value: 'qa', shadowed: false, masked: false, change: 'edited' },
      { key: 'vars.region', value: 'eu', shadowed: false, masked: false, change: 'added' },
    ]);
  });

  it('keeps one edit per key, the last', () => {
    const edits = withEdit([{ key: 'profile', value: 'qa' }], { key: 'profile', value: 'dev' });
    expect(edits).toEqual([{ key: 'profile', value: 'dev' }]);
  });
});

describe('scopeOf', () => {
  it('is the project on Config, Alerts and Plugins, you on Your settings, neither elsewhere', () => {
    expect(
      ['/settings', '/settings/', '/settings/alerts', '/settings/plugins'].map(scopeOf),
    ).toEqual(['project', 'project', 'project', 'project']);
    expect(scopeOf('/settings/user')).toBe('user');
    expect(scopeOf('/settings/appearance')).toBeUndefined();
  });
});
