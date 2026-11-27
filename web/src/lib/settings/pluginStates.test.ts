import { describe, expect, it } from 'vitest';
import { hintParts, stateView } from './pluginStates.ts';

const plugin = {
  id: 'shopAuth',
  source: 'path:plugins/shop-auth',
  state: 'ready',
  detail: 'ready',
};

describe('stateView', () => {
  it('says ready in words, with no hint', () => {
    expect(stateView(plugin)).toEqual({ label: 'Ready', tone: 'vc-pass', hint: [] });
  });

  it('says not approved, and names the command that approves it as code', () => {
    const view = stateView({ ...plugin, state: 'notApproved', detail: 'not approved: x' });
    expect(view.label).toBe('Not approved');
    expect(view.tone).toBe('vc-blocked');
    expect(view.hint).toEqual([
      { text: 'Not approved yet. Run ', code: false },
      { text: 'vc plugin add path:plugins/shop-auth', code: true },
      { text: ' to approve it.', code: false },
    ]);
  });

  it('removes a plugin only the lock has by its id', () => {
    const view = stateView({ ...plugin, state: 'notInConfig', detail: '' });
    expect(view.hint.find((part) => part.code)?.text).toBe('vc plugin remove shopAuth');
  });

  it('falls back to the engine detail, capitalised, for a word without its own hint', () => {
    const detail = 'its entry program `node` cannot be found: install it';
    const view = stateView({ ...plugin, state: 'noEntry', detail });
    expect(view.label).toBe('No entry program');
    expect(view.hint).toEqual([
      { text: 'Its entry program ', code: false },
      { text: 'node', code: true },
      { text: ' cannot be found: install it', code: false },
    ]);
  });

  it('turns an unknown camelCase word into words', () => {
    expect(stateView({ ...plugin, state: 'brandNewState', detail: '' }).label).toBe(
      'Brand new state',
    );
  });
});

describe('hintParts', () => {
  it('keeps text without backticks as one plain run', () => {
    expect(hintParts('Nothing to run.')).toEqual([{ text: 'Nothing to run.', code: false }]);
  });
});
