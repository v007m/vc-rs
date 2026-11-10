import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';

requiresVcUi();

test('shows the approval queue, the connect steps and the activity log', async ({ page }) => {
  await page.goto('/agents');
  const queue = page.getByRole('region', { name: 'Waiting for you' });
  await expect(queue).toBeVisible();
  await expect(queue.getByText('Nothing waiting.')).toBeVisible();
  await expect(page.getByRole('region', { name: 'Connect an agent' })).toContainText('vc mcp');
  await expect(page.getByRole('region', { name: 'Activity' })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('switches the connect snippet with the keyboard', async ({ page }) => {
  await page.goto('/agents');
  const cursor = page.getByRole('group', { name: 'Agent' }).getByRole('button', { name: 'Cursor' });
  await cursor.focus();
  await page.keyboard.press('Enter');
  await expect(cursor).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('region', { name: 'Connect an agent' })).toContainText('mcpServers');
});
