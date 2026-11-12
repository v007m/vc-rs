import { expect, test, type Page } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

/** Runs the seed project once, so the history has a run to show. */
async function runOnce(page: Page): Promise<void> {
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run all tests' }).click();
  await expect(page.getByRole('status')).toHaveText(/Passed|Failed/, { timeout: 30_000 });
}

test('shows pass rate and duration per stored run', async ({ page }) => {
  await runOnce(page);
  await page.goto('/insights');
  const passRate = page.getByRole('region', { name: 'Pass rate per run' });
  await expect(passRate.getByRole('img', { name: /run #\d+: \d+% passed/i }).first()).toBeVisible();
  const duration = page.getByRole('region', { name: 'Run duration' });
  await expect(duration.getByRole('img', { name: /run #\d+ took/i }).first()).toBeVisible();
});

test('lists the flakiest tests, or says there are none', async ({ page }) => {
  await runOnce(page);
  await page.goto('/insights');
  const flaky = page.getByRole('region', { name: 'Flakiest tests' });
  await expect(flaky.getByText(/No flaky or failing tests|users\.crud#/).first()).toBeVisible();
});

test('shows the slowest apis by p50 and p95', async ({ page }) => {
  await runOnce(page);
  await page.goto('/insights');
  const slowest = page.getByRole('region', { name: 'Slowest APIs' });
  await expect(
    slowest.getByRole('img', { name: /#\w+: p50 [\d.]+ m?s, p95 / }).first(),
  ).toBeVisible();
});

test('narrows the window with the time range', async ({ page }) => {
  await runOnce(page);
  await page.goto('/insights');
  const range = page.getByRole('group', { name: 'Time range' });
  await range.getByRole('button', { name: 'Last 10 runs' }).click();
  await expect(range.getByRole('button', { name: 'Last 10 runs' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
});

test('lists every api with its tests and statuses', async ({ page }) => {
  await runOnce(page);
  await page.goto('/insights');
  const coverage = page.getByRole('region', { name: 'API coverage' });
  await expect(coverage.getByRole('rowheader', { name: /\w+#\w+/ }).first()).toBeVisible();
  // A status chip's screen-reader text; the legend has no colon.
  await expect(coverage.getByText(/: documented and seen;/).first()).toBeAttached();
});
