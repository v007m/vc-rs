import { describe, expect, it } from 'vitest';
import {
  CONFIG_PATH,
  draftOf,
  editsBetween,
  pointerOf,
  profileProblem,
  varProblem,
  type ConfigDraft,
} from './configEdits.ts';
import { MASKED, type ConfigValue } from './port.ts';

const layer = (value: string, source = CONFIG_PATH) => ({ value, source });

const VALUES: ConfigValue[] = [
  { key: 'defaults.timeout', layers: [layer('5s', 'profile dev'), layer('10s')] },
  { key: 'defaults.concurrency', layers: [layer('8', 'built-in')] },
  { key: 'vars.usersUrl', layers: [layer('http://localhost:8080')] },
  { key: 'vars.token', layers: [layer(MASKED)] },
  { key: 'vars.fromEnv', layers: [layer('x', '.env')] },
];

function base(): ConfigDraft {
  return draftOf('shop', VALUES, ['dev', 'staging', 'qa'], 'dev');
}

describe('draftOf', () => {
  it('keeps only what the config file itself sets, and the profiles in file order', () => {
    expect(base()).toEqual({
      project: 'shop',
      fields: { 'defaults.timeout': '10s', 'defaults.concurrency': '', 'defaults.perHost': '' },
      vars: { usersUrl: 'http://localhost:8080', token: MASKED },
      profiles: [
        { id: 'dev', isDefault: true, at: 0 },
        { id: 'staging', isDefault: false, at: 1 },
        { id: 'qa', isDefault: false, at: 2 },
      ],
    });
  });
});

describe('editsBetween', () => {
  it('gives nothing for an unchanged draft', () => {
    expect(editsBetween(base(), base())).toEqual([]);
  });

  it('sets the name and defaults, numbers as numbers, and removes a cleared one', () => {
    const draft = base();
    draft.project = ' store ';
    draft.fields['defaults.timeout'] = '';
    draft.fields['defaults.concurrency'] = '16';
    draft.fields['defaults.perHost'] = 'four';
    expect(editsBetween(base(), draft)).toEqual([
      { pointer: '/project', op: 'set', value: 'store' },
      { pointer: '/defaults/timeout', op: 'remove' },
      { pointer: '/defaults/concurrency', op: 'set', value: 16 },
      { pointer: '/defaults/perHost', op: 'set', value: 'four' },
    ]);
  });

  it('removes and sets vars, never writing the mask back', () => {
    const draft = base();
    draft.vars = { token: MASKED, 'a/b~c': 'v' };
    expect(editsBetween(base(), draft)).toEqual([
      { pointer: '/vars/usersUrl', op: 'remove' },
      { pointer: '/vars/a~1b~0c', op: 'set', value: 'v' },
    ]);
    const removed = { ...base(), vars: { usersUrl: 'http://localhost:8080' } };
    expect(editsBetween(base(), removed)).toEqual([{ pointer: '/vars/token', op: 'remove' }]);
  });

  it('orders profile edits so each pointer still holds', () => {
    const draft = base();
    draft.profiles = [
      { id: 'dev', isDefault: false, at: 0 },
      { id: 'qa', isDefault: false, at: 2 },
      { id: 'eu', isDefault: true },
    ];
    draft.profiles.push({ id: 'us', isDefault: false });
    expect(editsBetween(base(), draft)).toEqual([
      { pointer: '/profiles/0/default', op: 'set', value: false },
      { pointer: '/profiles/1', op: 'remove' },
      { pointer: '/profiles/-', op: 'insert', value: { id: 'eu', default: true } },
      { pointer: '/profiles/-', op: 'insert', value: { id: 'us' } },
    ]);
    const dropped = { ...base(), profiles: [] };
    expect(editsBetween(base(), dropped).map(({ pointer }) => pointer)).toEqual([
      '/profiles/2',
      '/profiles/1',
      '/profiles/0',
    ]);
  });
});

describe('names', () => {
  it('maps dotted keys to pointers', () => {
    expect(pointerOf('reports.html.layout')).toBe('/reports/html/layout');
    expect(pointerOf('vars.a.b')).toBe('/vars/a.b');
  });

  it('says why a profile or var name will not do', () => {
    const profiles = base().profiles;
    expect(profileProblem('eu-west', profiles)).toBeUndefined();
    expect(profileProblem('Staging', profiles)).toContain('lowercase');
    expect(profileProblem('dev', profiles)).toBe('There is a profile dev already.');
    expect(varProblem('ordersUrl', base().vars)).toBeUndefined();
    expect(varProblem('orders url', base().vars)).toContain('letters, digits');
    expect(varProblem('token', base().vars)).toBe('There is a variable token already.');
  });
});
