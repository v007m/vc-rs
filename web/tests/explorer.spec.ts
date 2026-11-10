import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('lists the project tests as a tree', async ({ page }) => {
  await page.goto('/tests');
  const tree = page.getByRole('tree', { name: 'Tests' });
  await expect(tree).toBeVisible();
  await expect(tree.getByRole('treeitem').first()).toBeVisible();
});
