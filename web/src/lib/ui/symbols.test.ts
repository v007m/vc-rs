import { expect, it } from 'vitest';
import { play } from './symbols.ts';

it('runs an effect once, restarting it, and clears it when the animation ends', () => {
  const el = document.createElement('span');
  play(el, 'wiggle');
  play(el, 'wiggle');
  expect(el.classList.contains('vc-sym-wiggle')).toBe(true);
  el.dispatchEvent(new Event('animationend'));
  expect(el.classList.contains('vc-sym-wiggle')).toBe(false);
  expect(() => play(undefined, 'bounce')).not.toThrow();
});
