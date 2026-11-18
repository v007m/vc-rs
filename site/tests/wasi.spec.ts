import { readFileSync } from 'node:fs';

import { expect, test } from '@playwright/test';

import { Wasi, due, now } from '../src/components/playground/wasi';

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

// The real module, as the worker runs it: the demo's `/slow` waits on the page's clock (D329).
const MODULE = new URL('../public/playground/vc-playground.wasm', import.meta.url);
const SLOW = {
  'vibranium.config.yaml': 'syntaxVersion: 2\nprofiles:\n  - {id: local, default: true}\nvars:\n  demoUrl: http://localhost:7421\n',
  'services/demo/demo.api.yaml':
    'baseUrl: ${var.demoUrl}\napis:\n  - id: slow\n    path: /slow\n    query: {ms: \'300\'}\n',
  'services/demo/slow.test.yaml': 'tests:\n  - id: waits\n    call: demo#slow\n    expect: {body: {match: {sleptMs: 300}}}\n',
};

test('the demo waits as long as /slow asks in the playground too', async () => {
  const module = await WebAssembly.compile(readFileSync(MODULE));
  let said = '';
  const stdin = new TextEncoder().encode(JSON.stringify({ files: SLOW, command: 'run' }));
  const wasi = new Wasi({ stdin, write: (_stream, text) => (said += text) });
  const instance = await WebAssembly.instantiate(module, wasi.imports(module));
  const started = performance.now();
  const exit = wasi.start(instance);
  const took = performance.now() - started;
  expect(exit, said).toBe(0);
  expect(said).toContain('1 passed');
  expect(took).toBeGreaterThanOrEqual(300);
});
