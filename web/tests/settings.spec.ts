import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('shows each setting with its value and the layer that set it', async ({ page }) => {
  await page.goto('/settings');
  const table = page.getByRole('table', { name: 'Effective settings' });
  // The key is the row's header, so its cells are the value, then the layer that set it.
  const timeout = table.getByRole('row', { name: /defaults\.timeout/ }).getByRole('cell');
  await expect(timeout.nth(0)).toHaveText('30s');
  await expect(timeout.nth(1)).toContainText('built-in');
  const mockUrl = table.getByRole('row', { name: /vars\.mockUrl/ }).getByRole('cell');
  await expect(mockUrl.nth(0)).toHaveText('http://127.0.0.1:18080');
  await expect(mockUrl.nth(1)).toContainText('vibranium.config.yaml');
});

test('explains the settings under the chosen profile', async ({ page }) => {
  await page.goto('/settings');
  await expect(page.getByRole('combobox', { name: 'Profile' })).toHaveValue('dev');
  await expect(
    page.getByRole('table', { name: 'Effective settings' }).getByRole('row', { name: /^profile/ }),
  ).toContainText('dev');
});

test('opens the config in the editor', async ({ page }) => {
  await page.goto('/settings');
  await page.getByRole('link', { name: 'Open vibranium.config.yaml in the editor' }).click();
  await expect(page).toHaveURL(/\/editor\?file=vibranium\.config\.yaml$/);
});
