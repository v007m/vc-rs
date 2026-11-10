import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('shows the approval queue and the agent log', async ({ page }) => {
  await page.goto('/agents');
  await expect(page.getByRole('region', { name: 'Approvals' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Agent log' })).toBeVisible();
});
