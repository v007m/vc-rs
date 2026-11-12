import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('runs every test of the seed project and shows the outcome', async ({ page }) => {
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run all tests' }).click();
  await expect(page.getByRole('status')).toHaveText(/Passed|Failed/, { timeout: 30_000 });
  await expect(page.getByRole('img', { name: /of 3 tests/ })).toBeVisible();
  // The log names a test by its label: the full ID, plus the row and repeat it ran as.
  await expect(page.getByRole('log', { name: 'Run events, newest first' })).toContainText(
    'users.crud#listsUsers passed',
  );
});

test('filters the event log by level', async ({ page }) => {
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run all tests' }).click();
  await expect(page.getByRole('status')).toHaveText(/Passed|Failed/, { timeout: 30_000 });
  await page.getByRole('group', { name: 'Level' }).getByRole('button', { name: 'Errors' }).click();
  await expect(page.getByRole('button', { name: 'Errors' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByRole('log')).not.toContainText('started:');
});

test('lists the finished run in recent runs and opens it from the history', async ({ page }) => {
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run all tests' }).click();
  await expect(page.getByRole('status')).toHaveText(/Passed|Failed/, { timeout: 30_000 });
  // Other workers share this vc ui: the newest run may still be going, or be a Client call
  // (adhoc) that keeps no tests. Open a finished run of tests.
  const recent = page.getByRole('list', { name: 'Recent runs' });
  await recent
    .getByRole('button', { name: /^Run #\d+, (passed|failed)$/ })
    .filter({ hasNotText: 'adhoc' })
    .first()
    .click();
  await expect(page.getByRole('heading', { name: /^Run #\d+$/ })).toBeVisible();
  await expect(page.getByRole('listbox', { name: 'Tests' })).toContainText('users.crud#listsUsers');
  await page.getByRole('button', { name: 'Back to the current run' }).click();
  await expect(page.getByRole('log', { name: 'Run events, newest first' })).toBeVisible();
});

test('opens the ended run from the history, in the app', async ({ page }) => {
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run all tests' }).click();
  await expect(page.getByRole('status')).toHaveText(/Passed|Failed/, { timeout: 30_000 });
  const title = await page.getByRole('heading', { name: /^Run #\d+$/ }).textContent();
  // The seed project writes no HTML report, so no path is named.
  await expect(page.getByText('HTML report')).toHaveCount(0);
  await page.getByRole('button', { name: 'Open report' }).click();
  await expect(page.getByRole('button', { name: 'Back to the current run' })).toBeVisible();
  await expect(page.getByRole('heading', { name: title ?? '' })).toBeVisible();
  await expect(page.getByRole('listbox', { name: 'Tests' })).toContainText('users.crud#listsUsers');
});
