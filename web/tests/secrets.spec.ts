import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';
import { startVcUi, type VcUi } from './vcui';

requiresVcUi();

/**
 * Its own `vc ui` on a port the OS picks, so the declared secrets stay out of the seed project the
 * other specs share.
 */
const port = '0';
/** Test data, in no real provider's key format. */
const token = 'tok-e2e-3a9c51';

/** Declares two secrets and gives `.env` a value for one of them. */
async function declareSecrets(projectDir: string): Promise<void> {
  await writeFile(
    join(projectDir, '.env.schema'),
    '# The shop token\nE2E_SHOP_TOKEN=\nE2E_MISSING=\n',
  );
  await writeFile(join(projectDir, '.env'), `E2E_SHOP_TOKEN=${token}\n`);
}

let vcUi: VcUi | undefined;
test.beforeAll(async () => {
  vcUi = await startVcUi(port, declareSecrets);
});
test.afterAll(async () => {
  await vcUi?.stop();
});

test('says whether each declared secret resolves under each profile, never its value', async ({
  browser,
}) => {
  if (!vcUi) throw new Error('vc ui did not start');
  const page = await (await browser.newContext()).newPage();
  const bodies: string[] = [];
  page.on('response', async (response) => {
    if (response.url().includes('/CheckSecrets')) bodies.push((await response.body()).toString());
  });
  await page.goto(vcUi.printed.launchUrl);
  await page.goto(`${vcUi.origin}/settings`);
  await expect(page.getByLabel('E2E_SHOP_TOKEN on dev: resolved')).toBeVisible();
  await expect(page.getByLabel('E2E_MISSING on dev: not set')).toBeVisible();
  await page.getByRole('button', { name: 'Check secrets' }).click();
  await expect(page.getByLabel('E2E_MISSING on dev: not set')).toBeVisible();
  await expect(page.locator('body')).not.toContainText(token);
  expect(bodies.length).toBeGreaterThan(0);
  expect(bodies.some((body) => body.includes(token))).toBe(false);
});
