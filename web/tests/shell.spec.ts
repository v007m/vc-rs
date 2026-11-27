import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

const spaces = [
  { label: 'Client', path: '/client', title: 'API client' },
  { label: 'Tests', path: '/tests', title: 'Tests' },
  { label: 'Runs', path: '/runs', title: 'Runs' },
  { label: 'Agents', path: '/agents', title: 'AI agents' },
  { label: 'Settings', path: '/settings', title: 'Settings' },
];

test('opens in Client', async ({ page }) => {
  await page.goto('/');
  await expect(page).toHaveURL(/\/client$/);
});

test('old links redirect, keeping their query', async ({ page }) => {
  await page.goto('/editor?file=vibranium.config.yaml');
  await expect(page).toHaveURL(/\/tests\/editor\?file=vibranium\.config\.yaml$/);
  await page.goto('/insights');
  await expect(page).toHaveURL(/\/runs\/overview$/);
});

for (const { label, path, title } of spaces) {
  test(`the rail opens ${label} and marks it current`, async ({ page }) => {
    await page.goto('/agents');
    const rail = page.getByRole('navigation', { name: 'Spaces' });
    await rail.getByRole('link', { name: label, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`${path}$`));
    await expect(rail.getByRole('link', { name: label, exact: true })).toHaveAttribute(
      'aria-current',
      'page',
    );
    await expect(page.getByRole('heading', { level: 1, name: title })).toBeVisible();
  });
}

test('the sidebar hides and shows again', async ({ page }) => {
  await page.goto('/tests');
  await page.getByRole('button', { name: 'Hide sidebar' }).click();
  await expect(page.getByRole('complementary', { name: 'Tests sidebar' })).toBeHidden();
  await page.getByRole('button', { name: 'Show sidebar' }).click();
  await expect(page.getByRole('complementary', { name: 'Tests sidebar' })).toBeVisible();
});

test('announces a route change from a live region kept out of sight', async ({ page }) => {
  await page.goto('/tests');
  await page
    .getByRole('navigation', { name: 'Spaces' })
    .getByRole('link', { name: 'Runs' })
    .click();
  const announcer = page.locator('#svelte-announcer');
  await expect(announcer).toHaveAttribute('aria-live', 'assertive');
  await expect(page).toHaveTitle('Runs · Vibranium');
  await expect(announcer).toHaveText('Runs · Vibranium');
  // The CSP blocks SvelteKit's own style attribute; shell.css hides it instead.
  expect(await announcer.boundingBox()).toMatchObject({ width: 1, height: 1 });
});
