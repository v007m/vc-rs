import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

const screens = [
  { label: 'Tests', path: '/tests' },
  { label: 'Editor', path: '/editor' },
  { label: 'Runs', path: '/runs' },
  { label: 'AI agents', path: '/agents' },
];

test('opens on the Tests screen', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/tests$/);
});

for (const { label, path } of screens) {
  test(`the nav opens ${label} and marks it current`, async ({ page }) => {
    await page.goto('/tests');
    const nav = page.getByRole('navigation', { name: 'Main' });
    await nav.getByRole('link', { name: label }).click();
    await expect(page).toHaveURL(new RegExp(`${path}$`));
    await expect(nav.getByRole('link', { name: label })).toHaveAttribute('aria-current', 'page');
    await expect(page.getByRole('heading', { level: 1, name: label })).toBeVisible();
  });
}

test('the sidebar collapses to an icon rail and expands again', async ({ page }) => {
  await page.goto('/tests');
  await page.getByRole('button', { name: 'Collapse sidebar' }).click();
  await expect(page.getByRole('link', { name: 'Tests' }).locator('.label')).toBeHidden();
  await page.getByRole('button', { name: 'Expand sidebar' }).click();
  await expect(page.getByRole('link', { name: 'Tests' }).locator('.label')).toBeVisible();
});
