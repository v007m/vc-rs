import { expect, it } from 'vitest';
import { tabMove } from './tabs.ts';

const tabs = [
  { id: 'a', label: 'A' },
  { id: 'b', label: 'B' },
  { id: 'c', label: 'C' },
];

it('moves with the arrow keys, wrapping, and jumps with Home and End', () => {
  expect(tabMove(tabs, 'a', 'ArrowRight')).toBe('b');
  expect(tabMove(tabs, 'c', 'ArrowRight')).toBe('a');
  expect(tabMove(tabs, 'a', 'ArrowLeft')).toBe('c');
  expect(tabMove(tabs, 'b', 'End')).toBe('c');
  expect(tabMove(tabs, 'c', 'Home')).toBe('a');
  expect(tabMove(tabs, 'a', 'Enter')).toBeUndefined();
});
