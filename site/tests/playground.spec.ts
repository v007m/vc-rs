import { expect, test } from '@playwright/test';

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
