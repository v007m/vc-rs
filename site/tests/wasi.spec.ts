import { expect, test } from '@playwright/test';

import { due, now } from '../src/components/playground/wasi';

// `poll_oneoff`'s clock subscriptions, without a page: each is waited for on the monotonic clock,
// so an absolute wall-clock deadline is moved by how far it is from the wall clock now.

const at = { realtime: 1_700_000_000_000_000_000n, monotonic: 5_000n };

test('a relative timeout is due that long after now, on either clock', () => {
  expect(due({ clock: 1, timeout: 300n, absolute: false }, at)).toBe(5_300n);
  expect(due({ clock: 0, timeout: 400n, absolute: false }, at)).toBe(5_400n);
});

test('an absolute monotonic deadline is due as it is', () => {
  expect(due({ clock: 1, timeout: 9_000n, absolute: true }, at)).toBe(9_000n);
});

test('an absolute wall-clock deadline is due as far ahead as it is of the wall clock', () => {
  expect(due({ clock: 0, timeout: at.realtime + 2_000n, absolute: true }, at)).toBe(7_000n);
  expect(due({ clock: 0, timeout: at.realtime - 2_000n, absolute: true }, at)).toBe(3_000n);
});

test('the wall clock is Date.now() and the monotonic clock is performance.now()', () => {
  const wall = BigInt(Date.now()) * 1_000_000n;
  const read = now(0);
  expect(read >= wall && read - wall < 1_000_000_000n).toBe(true);
  expect(now(1) < wall).toBe(true);
});
