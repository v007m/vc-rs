import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

const crud = 'services/users/crud.test.yaml';

test('opens a project file from the tree', async ({ page }) => {
  await page.goto('/editor');
  const tree = page.getByRole('tree', { name: 'Files' });
  await tree.getByRole('treeitem', { name: 'crud.test.yaml' }).click();
  await expect(page.getByRole('textbox', { name: crud })).toHaveValue(/id:/);
  await expect(page.getByRole('region', { name: 'Problems' })).toContainText('0 errors');
});

test('saves an edit and keeps it after a reload', async ({ page }) => {
  await page.goto(`/editor?file=${encodeURIComponent(crud)}`);
  const text = page.getByRole('textbox', { name: crud });
  await expect(text).toHaveValue(/id:/);
  await text.press('ControlOrMeta+End');
  await text.pressSequentially('# saved by e2e\n');
  await expect(page.getByText('Unsaved changes')).toBeVisible();
  await page.keyboard.press('ControlOrMeta+s');
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByRole('textbox', { name: crud })).toHaveValue(/# saved by e2e\n$/);
});

test('shows the diff of unsaved edits and reverts them', async ({ page }) => {
  await page.goto(`/editor?file=${encodeURIComponent(crud)}`);
  const text = page.getByRole('textbox', { name: crud });
  await expect(text).toHaveValue(/id:/);
  await text.press('ControlOrMeta+End');
  await text.pressSequentially('# draft\n');
  await page.getByRole('button', { name: /^Diff/ }).click();
  const diff = page.getByRole('region', { name: 'Changes since the last save' });
  await expect(diff).toContainText('# draft');
  await page.getByRole('button', { name: 'Revert' }).click();
  await expect(diff).toContainText('No changes since the last save.');
});

test('opens the editor from a test in the Tests screen', async ({ page }) => {
  await page.goto('/tests');
  await page.getByRole('tree', { name: 'Tests' }).getByRole('treeitem').nth(1).click();
  await page.getByRole('link', { name: 'Open in editor' }).click();
  await expect(page.getByRole('textbox', { name: crud })).toBeVisible();
});
