import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('runs the project and shows the outcome', async ({ page }) => {
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run' }).click();
  await expect(page.getByRole('status')).toContainText(/passed|failed/);
});
