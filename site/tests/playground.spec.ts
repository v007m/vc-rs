import { expect, test } from '@playwright/test';

import { Wasi } from '../src/components/playground/wasi';

// The playground runs the real `vc` in the page (D293): the starter project passes, and a broken
// value fails with the excerpt and diagnostic code `vc run` prints.

const output = (page: import('@playwright/test').Page) => page.getByRole('log', { name: 'vc output' });

test('the starter project passes', async ({ page }) => {
  await page.goto('/vc-rs/playground/');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.getByText(/^exit \d+$/)).toHaveText('exit 0', { timeout: 60_000 });
  await expect(output(page)).toContainText('plan  10 tests (14 instances) in 5 files');
  // Each file's tests print under its name, once (D321).
  await expect(output(page)).toContainText('Sign-up rules');
  await expect(output(page)).toContainText('14 passed');
});

test('a broken value shows the failure excerpt and its code', async ({ page }) => {
  await page.goto('/vc-rs/playground/');
  await page.getByRole('tab', { name: /Query params/ }).click();
  const line = page.locator('.cm-line', { hasText: 'q: ${example.term}' });
  await line.click();
  await page.keyboard.press('End');
  await page.keyboard.press('Shift+Home');
  await page.keyboard.type('q: boots');
  await page.keyboard.press('Escape');
  await page.getByRole('tab', { name: /CRUD test/ }).click();
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.getByText(/^exit \d+$/)).toHaveText('exit 1', { timeout: 60_000 });
  await expect(output(page)).toContainText('error[V5101]: expect.body.match.query');
  await expect(output(page)).toContainText('q: boots');
  await expect(output(page)).toContainText('pass `--seed');
});

test('stop ends a run that has not finished, and the next run starts afresh', async ({ page }) => {
  // Hold the module's download so the first run is still loading when Stop is pressed.
  let release = () => {};
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.context().route('**/vc-playground.wasm', async (route) => {
    await held;
    await route.continue();
  });
  await page.goto('/vc-rs/playground/');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await expect(output(page)).toContainText('Stopped.');
  await expect(page.getByRole('button', { name: 'Run', exact: true })).toBeEnabled();
  await expect(page.getByText('Running…')).toHaveCount(0);
  release();
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.getByText(/^exit \d+$/)).toHaveText('exit 0', { timeout: 60_000 });
});

// The module the site serves, run as the worker runs it: the demo's `/slow` waits on the clock
// (D329). Like the tests above, it needs the built site, and fails when the module is missing.
const MODULE = '/vc-rs/playground/vc-playground.wasm';
const SLOW = {
  'vibranium.config.yaml': 'syntaxVersion: 2\nprofiles:\n  - {id: local, default: true}\nvars:\n  demoUrl: http://localhost:7421\n',
  'services/demo/demo.api.yaml':
    'baseUrl: ${var.demoUrl}\napis:\n  - id: slow\n    path: /slow\n    query: {ms: \'300\'}\n',
  'services/demo/slow.test.yaml': 'tests:\n  - id: waits\n    call: demo#slow\n    expect: {body: {match: {sleptMs: 300}}}\n',
};

test('the demo waits as long as /slow asks in the playground too', async ({ request }) => {
  const served = await request.get(MODULE);
  expect(served.ok(), `${MODULE}: ${served.status()}`).toBe(true);
  const module = await WebAssembly.compile(await served.body());
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
