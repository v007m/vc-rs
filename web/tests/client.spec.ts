import { fileURLToPath } from 'node:url';
import { expect, test, type Page } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

const har = fileURLToPath(new URL('fixtures/har/users-session.har', import.meta.url));

/** Opens the API client on one api of the seed project. */
async function openApi(page: Page, name: string): Promise<void> {
  await page.goto('/client');
  await page.getByRole('tree', { name: 'Apis' }).getByRole('treeitem', { name }).click();
}

test('calls an api and shows the status and body', async ({ page }) => {
  await openApi(page, 'List users');
  await page.getByRole('button', { name: 'Send' }).click();
  const response = page.getByRole('region', { name: 'Response' });
  await expect(response.getByText(/^200\b/)).toBeVisible({ timeout: 15_000 });
  await expect(response.getByRole('tab', { name: 'Body', selected: true })).toBeVisible();
});

test('saves a call with a var as a test expecting a body field', async ({ page }) => {
  await openApi(page, 'Create a user');
  await page.getByRole('tab', { name: 'Vars' }).click();
  await page.getByRole('textbox', { name: 'email' }).fill('grace@example.com');
  await page.getByRole('button', { name: 'Send' }).click();
  const response = page.getByRole('region', { name: 'Response' });
  await expect(response.getByText(/^201\b/)).toBeVisible({ timeout: 15_000 });

  await response.getByRole('button', { name: 'Save as test' }).click();
  const dialog = page.getByRole('dialog', { name: 'Save as test' });
  await dialog.getByRole('textbox', { name: 'Test ID' }).fill('users.crud#createsGrace');
  await dialog.getByRole('checkbox', { name: 'body.email' }).check();
  await dialog.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByRole('status')).toContainText('users.crud#createsGrace');

  await page.goto('/editor?file=services/users/crud.test.yaml');
  await expect(page.getByRole('textbox', { name: 'services/users/crud.test.yaml' })).toHaveValue(
    /id: createsGrace[\s\S]*email: grace@example\.com/,
  );
});

test('imports a HAR file with a flow test after a preview', async ({ page }) => {
  await page.goto('/client');
  await page.getByRole('button', { name: 'Import HAR' }).click();
  const dialog = page.getByRole('dialog', { name: 'Import a HAR file' });
  await dialog.getByLabel('HAR file').setInputFiles(har);
  await dialog.getByRole('textbox', { name: 'Flow test' }).fill('users.recorded#replaysSession');
  await dialog.getByRole('button', { name: 'Preview' }).click();

  const changes = dialog.getByRole('list', { name: 'Changes' });
  await expect(changes.getByText('services/users/recorded.test.yaml')).toBeVisible();
  await expect(dialog.getByText(/filtered 1 static/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Write files' }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole('group', { name: 'Mode' }).getByRole('button', { name: 'Flow' }).click();
  await expect(
    page.getByRole('list', { name: 'Flow tests' }).getByText('users.recorded#replaysSession'),
  ).toBeVisible();
});

test('reaches every part of the request from the keyboard', async ({ page }) => {
  await openApi(page, 'List users');
  const tabs = page.getByRole('tablist', { name: 'Request' });
  await tabs.getByRole('tab', { name: 'Vars' }).focus();
  await page.keyboard.press('ArrowRight');
  await expect(tabs.getByRole('tab', { selected: true })).not.toHaveAccessibleName('Vars');
  await expect(page.getByRole('button', { name: 'Send' })).toBeEnabled();
});
