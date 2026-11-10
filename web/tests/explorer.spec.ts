import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('lists the seed tests as a tree of files', async ({ page }) => {
  await page.goto('/tests');
  const tree = page.getByRole('tree', { name: 'Tests' });
  await expect(page.getByText('3 tests in 1 file')).toBeVisible();
  await expect(tree.getByRole('treeitem')).toHaveCount(4);
  await expect(tree.getByRole('treeitem', { name: /users\.crud/ })).toHaveAttribute(
    'aria-expanded',
    'true',
  );
});

test('moves through the tree with the keyboard', async ({ page }) => {
  await page.goto('/tests');
  const items = page.getByRole('tree', { name: 'Tests' }).getByRole('treeitem');
  await items.first().focus();
  await page.keyboard.press('ArrowDown');
  await expect(items.nth(1)).toBeFocused();
  await page.keyboard.press('Space');
  await expect(items.nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('region', { name: 'Selection' })).toContainText('1 selected');
});

test('filters by text and runs one test from its detail', async ({ page }) => {
  await page.goto('/tests');
  await page.getByRole('searchbox', { name: 'Filter tests with query syntax' }).fill('creates');
  const tree = page.getByRole('tree', { name: 'Tests' });
  await expect(tree.getByRole('treeitem')).toHaveCount(2);
  await tree.getByText('Creates a user with the given email').click();
  await expect(
    page.getByRole('heading', { name: 'Creates a user with the given email' }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Run this test' }).click();
  // The detail links to the run; a reload would drop the page's live run.
  await page.getByRole('link', { name: 'See it in Runs' }).click();
  await expect(page.getByRole('status')).toHaveText(/Running|Passed|Failed/, { timeout: 30_000 });
});
