import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('opens a project file in the editor', async ({ page }) => {
  await page.goto('/editor');
  await page.getByRole('tree', { name: 'Files' }).getByRole('treeitem').first().click();
  await expect(page.getByRole('textbox', { name: /\.yaml$/ })).toBeVisible();
});
