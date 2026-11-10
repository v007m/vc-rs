import { expect, test } from '@playwright/test';
import { callTool, e2eAgent } from './mcp';
import { fromSetup, requiresVcUi } from './support';

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

test('shows the /mcp URL and the agent token vc ui printed, masked until shown', async ({
  page,
}) => {
  const token = fromSetup('VC_E2E_AGENT_TOKEN');
  await page.goto('/agents');
  const connect = page.getByRole('region', { name: 'Connect an agent' });
  await connect.getByRole('button', { name: 'HTTP' }).click();
  await expect(connect).toContainText(fromSetup('VC_E2E_MCP_URL'));
  await expect(connect).not.toContainText(token);
  await connect.getByRole('button', { name: 'Show token' }).click();
  await expect(connect.getByText(token, { exact: true })).toBeVisible();
});

test('keeps a call and a run made in the browser out of the agent log', async ({ page }) => {
  await page.goto('/client');
  await page
    .getByRole('tree', { name: 'Apis' })
    .getByRole('treeitem', { name: 'List users' })
    .click();
  await page.getByRole('button', { name: 'Send' }).click();
  const response = page.getByRole('region', { name: 'Response' });
  await expect(response.getByText(/^200\b/)).toBeVisible({ timeout: 15_000 });
  await page.goto('/runs');
  await page.getByRole('button', { name: 'Run all tests' }).click();
  await expect(page.getByRole('status')).toHaveText(/Passed|Failed/, { timeout: 30_000 });

  await callTool('list_definitions');

  await page.goto('/agents');
  const log = page.getByRole('region', { name: 'Activity' });
  await expect(log.getByRole('button', { name: 'list_definitions' })).toBeVisible();
  await expect(log.getByRole('combobox', { name: 'Agent' })).toContainText(e2eAgent);
  await expect(log.getByRole('button', { name: 'call_api' })).toHaveCount(0);
  await expect(log.getByRole('button', { name: 'run_tests' })).toHaveCount(0);
});
