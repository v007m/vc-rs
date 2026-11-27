import { describe, expect, it } from 'vitest';
import { isRunKey } from './shortcut.ts';

const key = (key: string, mods: Partial<KeyboardEvent> = {}) => ({
  key,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe('isRunKey', () => {
  it('takes ⌘R and Ctrl+R, and leaves a reload with Shift and a bare R alone', () => {
    expect(isRunKey(key('r', { metaKey: true }))).toBe(true);
    expect(isRunKey(key('R', { ctrlKey: true }))).toBe(true);
    expect(isRunKey(key('r', { metaKey: true, shiftKey: true }))).toBe(false);
    expect(isRunKey(key('r'))).toBe(false);
  });
});
