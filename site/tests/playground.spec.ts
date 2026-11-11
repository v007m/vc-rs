import { expect, test } from '@playwright/test';

// The playground runs the real `vc` in the page (D293): the starter project passes, and a broken
// value fails with the excerpt and diagnostic code `vc run` prints.

const output = (page: import('@playwright/test').Page) => page.getByRole('log', { name: 'vc output' });

test('the starter project passes', async ({ page }) => {
  await page.goto('playground/');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.getByText(/^exit \d+$/)).toHaveText('exit 0', { timeout: 60_000 });
  await expect(output(page)).toContainText('plan  10 tests in 5 files');
  await expect(output(page)).toContainText('14 passed');
});

test('a broken value shows the failure excerpt and its code', async ({ page }) => {
  await page.goto('playground/');
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
