import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('shows each setting with its value and the layer that set it', async ({ page }) => {
  await page.goto('/settings');
  const table = page.getByRole('table', { name: 'Effective settings' });
  const timeout = table.getByRole('row', { name: /defaults\.timeout/ });
  await expect(timeout.getByRole('cell').nth(1)).toHaveText('30s');
  await expect(timeout.getByRole('cell').nth(2)).toContainText('built-in');
  const mockUrl = table.getByRole('row', { name: /vars\.mockUrl/ });
  await expect(mockUrl.getByRole('cell').nth(1)).toHaveText('http://127.0.0.1:18080');
  await expect(mockUrl.getByRole('cell').nth(2)).toContainText('vibranium.config.yaml');
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
